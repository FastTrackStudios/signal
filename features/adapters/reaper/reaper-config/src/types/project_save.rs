//! Project save/backup preferences (Preferences > Project > Save).

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::ini::IniFile;
use crate::parse::*;
use crate::types::common::AutoSaveMode;

const S: &str = "REAPER";

/// Project save, auto-save, and backup preferences.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Facet)]
pub struct ProjectSave {
    /// Auto-save interval in seconds.
    /// Key: `autosaveint`
    pub auto_save_interval: Option<u32>,

    /// When auto-save is triggered relative to transport state.
    /// Key: `autosavemode`
    #[facet(opaque)]
    pub auto_save_mode: Option<AutoSaveMode>,

    /// Auto-save backup retention limit.
    /// Key: `autosavebackuplimit`
    pub auto_save_backup_limit: Option<u32>,

    /// Backup file retention limit.
    /// Key: `savebackuplimit`
    pub save_backup_limit: Option<u32>,

    /// Project backup and save options bitfield.
    /// Key: `saveopts`
    pub save_options: Option<u32>,

    /// New filename options.
    /// Key: `newfnopt`
    pub new_filename_options: Option<u32>,
}

impl ProjectSave {
    pub(crate) fn from_ini(ini: &IniFile) -> Self {
        Self {
            auto_save_interval: get_u32(ini, S, "autosaveint"),
            auto_save_mode: get_enum_u32(ini, S, "autosavemode"),
            auto_save_backup_limit: get_u32(ini, S, "autosavebackuplimit"),
            save_backup_limit: get_u32(ini, S, "savebackuplimit"),
            save_options: get_u32(ini, S, "saveopts"),
            new_filename_options: get_u32(ini, S, "newfnopt"),
        }
    }

    /// Returns a new [`ProjectSave`] with REAPER factory default values.
    pub fn factory_defaults() -> Self {
        Self {
            auto_save_interval: Some(15),                         // autosaveint=15
            auto_save_mode: Some(AutoSaveMode::WhenNotRecording), // autosavemode=0
            auto_save_backup_limit: Some(50),                     // autosavebackuplimit=50
            save_backup_limit: Some(50),                          // savebackuplimit=50
            save_options: Some(12977),                            // saveopts=12977
            new_filename_options: None, // newfnopt not in factory defaults
        }
    }

    pub(crate) fn write_to(&self, ini: &mut IniFile) {
        set_opt_u32(ini, S, "autosaveint", self.auto_save_interval);
        set_opt_enum_u32(ini, S, "autosavemode", self.auto_save_mode);
        set_opt_u32(ini, S, "autosavebackuplimit", self.auto_save_backup_limit);
        set_opt_u32(ini, S, "savebackuplimit", self.save_backup_limit);
        set_opt_u32(ini, S, "saveopts", self.save_options);
        set_opt_u32(ini, S, "newfnopt", self.new_filename_options);
    }
}
