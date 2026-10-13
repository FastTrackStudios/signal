//! Media preferences (Preferences > Media).

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::ini::IniFile;
use crate::parse::*;
use crate::types::common::{AcidImportMode, InsertMasterTrackMode};

/// Controls when peak cache files are generated.
///
/// Maps to REAPER's `peakcachegenmode` INI key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Facet)]
pub struct PeakCacheGenFlags {
    /// Generate peak cache when importing media. Default: `true`.
    pub on_import: bool,
    /// Generate peak cache when recording new media. Default: `true`.
    pub on_record: bool,
}

impl Default for PeakCacheGenFlags {
    fn default() -> Self {
        Self {
            on_import: true,
            on_record: true,
        }
    }
}

impl PeakCacheGenFlags {
    /// Encode as the `peakcachegenmode` INI integer.
    pub fn to_u32(self) -> u32 {
        let mut v = 0u32;
        if self.on_import {
            v |= 1;
        }
        if self.on_record {
            v |= 2;
        }
        v
    }

    /// Decode from the `peakcachegenmode` INI integer.
    pub fn from_u32(v: u32) -> Self {
        Self {
            on_import: (v & 1) != 0,
            on_record: (v & 2) != 0,
        }
    }
}

const S: &str = "REAPER";

/// Media import/handling preferences.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Facet)]
pub struct Media {
    /// Media import copy and naming options bitfield.
    /// Key: `copyimpmedia`
    pub copy_imported_media: Option<u32>,

    /// Miscellaneous media options bitfield.
    /// Key: `miscopts`
    pub misc_options: Option<u32>,

    /// Offline FX inactivity setting.
    /// Key: `offlineinact`
    pub offline_inactive: Option<u32>,

    /// When to generate peak cache files.
    /// Key: `peakcachegenmode`
    pub peak_cache_gen_mode: Option<PeakCacheGenFlags>,

    /// Peak cache generation rate.
    /// Key: `peakcachegenrs`
    pub peak_cache_gen_rate: Option<u32>,

    /// Peak recording bit mode.
    /// Key: `peakrecbm`
    pub peak_record_bit_mode: Option<u32>,

    /// Copy prompt on open toggle.
    /// Key: `opencopyprompt`
    pub open_copy_prompt: Option<u32>,

    /// How to insert multiple imported media items across tracks.
    /// Key: `insertmtrack`
    #[facet(opaque)]
    pub insert_master_track: Option<InsertMasterTrackMode>,

    /// BPM import adjustment mode.
    /// Key: `bpmprojadj`
    pub bpm_project_adjust: Option<u32>,

    /// BPM import from filename.
    /// Key: `bpminfnimport`
    pub bpm_filename_import: Option<u32>,

    /// How to handle ACID-format media with embedded tempo during import.
    /// Key: `acidimport`
    #[facet(opaque)]
    pub acid_import: Option<AcidImportMode>,

    /// REX file import mode.
    /// Key: `reximport`
    pub rex_import: Option<u32>,

    // ── Default paths ──
    /// Default render output directory.
    /// Key: `defrenderpath`
    pub default_render_path: Option<String>,

    /// Default save project directory.
    /// Key: `defsavepath`
    pub default_save_path: Option<String>,

    // ── Default track settings ──
    /// Default hardware volume.
    /// Key: `defhwvol`
    pub default_hardware_volume: Option<f64>,

    /// Default send flags bitfield.
    /// Key: `defsendflag`
    pub default_send_flags: Option<u32>,

    /// Default send volume in dB.
    /// Key: `defsendvol`
    pub default_send_volume: Option<f64>,

    /// Default track recording flags bitfield.
    /// Key: `deftrackrecflags`
    pub default_track_record_flags: Option<u32>,

    /// Default track input source.
    /// Key: `deftrackrecinput`
    pub default_track_record_input: Option<String>,

    /// Default track volume in dB.
    /// Key: `deftrackvol`
    pub default_track_volume: Option<f64>,

    /// Default vertical zoom level.
    /// Key: `defvzoom`
    pub default_vertical_zoom: Option<u32>,

    /// New track creation flags bitfield.
    /// Key: `newtflag`
    pub new_track_flags: Option<u32>,

    /// Take lane display mode.
    /// Key: `takelanes`
    pub take_lanes: Option<u32>,

    /// Template editing cursor behavior bitfield.
    /// Key: `templateditcursor`
    pub template_edit_cursor: Option<u32>,

    /// Track wiring display options bitfield.
    /// Key: `wiring_options`
    pub wiring_options: Option<u32>,

    /// Multi-project tab settings bitfield.
    /// Key: `multiprojopt`
    pub multi_project_options: Option<u32>,

    /// Project menu folder option.
    /// Key: `pmfol`
    pub project_menu_folder: Option<bool>,

    /// Recent file project first.
    /// Key: `rfprojfirst`
    pub recent_file_project_first: Option<bool>,
}

impl Media {
    pub(crate) fn from_ini(ini: &IniFile) -> Self {
        Self {
            copy_imported_media: get_u32(ini, S, "copyimpmedia"),
            misc_options: get_u32(ini, S, "miscopts"),
            offline_inactive: get_u32(ini, S, "offlineinact"),
            peak_cache_gen_mode: get_u32(ini, S, "peakcachegenmode")
                .map(PeakCacheGenFlags::from_u32),
            peak_cache_gen_rate: get_u32(ini, S, "peakcachegenrs"),
            peak_record_bit_mode: get_u32(ini, S, "peakrecbm"),
            open_copy_prompt: get_u32(ini, S, "opencopyprompt"),
            insert_master_track: get_enum_u32(ini, S, "insertmtrack"),
            bpm_project_adjust: get_u32(ini, S, "bpmprojadj"),
            bpm_filename_import: get_u32(ini, S, "bpminfnimport"),
            acid_import: get_enum_u32(ini, S, "acidimport"),
            rex_import: get_u32(ini, S, "reximport"),
            default_render_path: get_string(ini, S, "defrenderpath"),
            default_save_path: get_string(ini, S, "defsavepath"),
            default_hardware_volume: get_f64(ini, S, "defhwvol"),
            default_send_flags: get_u32(ini, S, "defsendflag"),
            default_send_volume: get_f64(ini, S, "defsendvol"),
            default_track_record_flags: get_u32(ini, S, "deftrackrecflags"),
            default_track_record_input: get_string(ini, S, "deftrackrecinput"),
            default_track_volume: get_f64(ini, S, "deftrackvol"),
            default_vertical_zoom: get_u32(ini, S, "defvzoom"),
            new_track_flags: get_u32(ini, S, "newtflag"),
            take_lanes: get_u32(ini, S, "takelanes"),
            template_edit_cursor: get_u32(ini, S, "templateditcursor"),
            wiring_options: get_u32(ini, S, "wiring_options"),
            multi_project_options: get_u32(ini, S, "multiprojopt"),
            project_menu_folder: get_bool(ini, S, "pmfol"),
            recent_file_project_first: get_bool(ini, S, "rfprojfirst"),
        }
    }

    /// Returns a new [`Media`] with REAPER factory default values.
    ///
    /// Fields whose factory value is an empty string are left as `None`.
    pub fn factory_defaults() -> Self {
        Self {
            copy_imported_media: Some(1), // copyimpmedia=1
            misc_options: Some(0),        // miscopts=0
            offline_inactive: Some(1),    // offlineinact=1
            peak_cache_gen_mode: Some(PeakCacheGenFlags::default()), // peakcachegenmode=3 (on_import + on_record)
            peak_cache_gen_rate: Some(300),                          // peakcachegenrs=300
            peak_record_bit_mode: Some(0),                           // peakrecbm=0
            open_copy_prompt: Some(32),                              // opencopyprompt=32
            insert_master_track: Some(InsertMasterTrackMode::Prompt), // insertmtrack=3
            bpm_project_adjust: Some(6),                             // bpmprojadj=6
            bpm_filename_import: Some(18),                           // bpminfnimport=18
            acid_import: Some(AcidImportMode::AlwaysPrompt),         // acidimport=2
            rex_import: Some(9),                                     // reximport=9
            default_render_path: None,                               // defrenderpath= (empty)
            default_save_path: None,                                 // defsavepath= (empty)
            default_hardware_volume: Some(1.0),                      // defhwvol=1.00000000000000
            default_send_flags: Some(0),                             // defsendflag=0
            default_send_volume: Some(1.0),                          // defsendvol=1.00000000000000
            default_track_record_flags: Some(256),                   // deftrackrecflags=256
            default_track_record_input: Some("0".to_string()),       // deftrackrecinput=0
            default_track_volume: Some(1.0),                         // deftrackvol=1.00000000000000
            default_vertical_zoom: Some(6),                          // defvzoom=6
            new_track_flags: Some(16515),                            // newtflag=16515
            take_lanes: Some(1032),                                  // takelanes=1032
            template_edit_cursor: None,                              // templateditcursor= (empty)
            wiring_options: None, // wiring_options not in factory defaults
            multi_project_options: Some(0), // multiprojopt=0
            project_menu_folder: Some(true), // pmfol=1
            recent_file_project_first: Some(false), // rfprojfirst=0
        }
    }

    pub(crate) fn write_to(&self, ini: &mut IniFile) {
        set_opt_u32(ini, S, "copyimpmedia", self.copy_imported_media);
        set_opt_u32(ini, S, "miscopts", self.misc_options);
        set_opt_u32(ini, S, "offlineinact", self.offline_inactive);
        if let Some(f) = self.peak_cache_gen_mode {
            ini.set(S, "peakcachegenmode", &f.to_u32().to_string());
        }
        set_opt_u32(ini, S, "peakcachegenrs", self.peak_cache_gen_rate);
        set_opt_u32(ini, S, "peakrecbm", self.peak_record_bit_mode);
        set_opt_u32(ini, S, "opencopyprompt", self.open_copy_prompt);
        set_opt_enum_u32(ini, S, "insertmtrack", self.insert_master_track);
        set_opt_u32(ini, S, "bpmprojadj", self.bpm_project_adjust);
        set_opt_u32(ini, S, "bpminfnimport", self.bpm_filename_import);
        set_opt_enum_u32(ini, S, "acidimport", self.acid_import);
        set_opt_u32(ini, S, "reximport", self.rex_import);
        set_opt_string(ini, S, "defrenderpath", &self.default_render_path);
        set_opt_string(ini, S, "defsavepath", &self.default_save_path);
        set_opt_f64(ini, S, "defhwvol", self.default_hardware_volume);
        set_opt_u32(ini, S, "defsendflag", self.default_send_flags);
        set_opt_f64(ini, S, "defsendvol", self.default_send_volume);
        set_opt_u32(ini, S, "deftrackrecflags", self.default_track_record_flags);
        set_opt_string(ini, S, "deftrackrecinput", &self.default_track_record_input);
        set_opt_f64(ini, S, "deftrackvol", self.default_track_volume);
        set_opt_u32(ini, S, "defvzoom", self.default_vertical_zoom);
        set_opt_u32(ini, S, "newtflag", self.new_track_flags);
        set_opt_u32(ini, S, "takelanes", self.take_lanes);
        set_opt_u32(ini, S, "templateditcursor", self.template_edit_cursor);
        set_opt_u32(ini, S, "wiring_options", self.wiring_options);
        set_opt_u32(ini, S, "multiprojopt", self.multi_project_options);
        set_opt_bool(ini, S, "pmfol", self.project_menu_folder);
        set_opt_bool(ini, S, "rfprojfirst", self.recent_file_project_first);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ini::IniFile;
    use crate::types::common::{AcidImportMode, InsertMasterTrackMode};

    // ── PeakCacheGenFlags ─────────────────────────────────────────────────

    #[test]
    fn peak_cache_gen_flags_roundtrip() {
        assert_eq!(
            PeakCacheGenFlags::from_u32(0),
            PeakCacheGenFlags {
                on_import: false,
                on_record: false
            }
        );
        assert_eq!(
            PeakCacheGenFlags::from_u32(1),
            PeakCacheGenFlags {
                on_import: true,
                on_record: false
            }
        );
        assert_eq!(
            PeakCacheGenFlags::from_u32(2),
            PeakCacheGenFlags {
                on_import: false,
                on_record: true
            }
        );
        assert_eq!(
            PeakCacheGenFlags::from_u32(3),
            PeakCacheGenFlags {
                on_import: true,
                on_record: true
            }
        );
        assert_eq!(PeakCacheGenFlags::default().to_u32(), 3);
    }

    #[test]
    fn peak_cache_gen_flags_parse_write() {
        let ini = IniFile::parse("[REAPER]\npeakcachegenmode=3\n");
        let m = Media::from_ini(&ini);
        assert_eq!(
            m.peak_cache_gen_mode,
            Some(PeakCacheGenFlags {
                on_import: true,
                on_record: true
            })
        );
        let mut out = IniFile::parse("[REAPER]\n");
        m.write_to(&mut out);
        assert_eq!(out.get("REAPER", "peakcachegenmode"), Some("3"));
    }

    // ── InsertMasterTrackMode ─────────────────────────────────────────────

    #[test]
    fn insert_master_track_mode_parse() {
        for (raw, expected) in [
            (0u32, InsertMasterTrackMode::OneTrack),
            (1, InsertMasterTrackMode::AcrossTracks),
            (2, InsertMasterTrackMode::Auto),
            (3, InsertMasterTrackMode::Prompt),
        ] {
            let ini = IniFile::parse(&format!("[REAPER]\ninsertmtrack={raw}\n"));
            let m = Media::from_ini(&ini);
            assert_eq!(m.insert_master_track, Some(expected));
            let mut out = IniFile::parse("[REAPER]\n");
            m.write_to(&mut out);
            assert_eq!(
                out.get("REAPER", "insertmtrack"),
                Some(raw.to_string().as_str())
            );
        }
    }

    // ── AcidImportMode ────────────────────────────────────────────────────

    #[test]
    fn acid_import_mode_parse() {
        for (raw, expected) in [
            (0u32, AcidImportMode::AdjustToProject),
            (1, AcidImportMode::SourceTempo),
            (2, AcidImportMode::AlwaysPrompt),
        ] {
            let ini = IniFile::parse(&format!("[REAPER]\nacidimport={raw}\n"));
            let m = Media::from_ini(&ini);
            assert_eq!(m.acid_import, Some(expected));
            let mut out = IniFile::parse("[REAPER]\n");
            m.write_to(&mut out);
            assert_eq!(
                out.get("REAPER", "acidimport"),
                Some(raw.to_string().as_str())
            );
        }
    }
}
