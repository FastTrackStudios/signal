//! Recording preferences (Preferences > Recording).

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::ini::IniFile;
use crate::parse::*;

const S: &str = "REAPER";

/// Recording preferences.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Facet)]
pub struct Recording {
    /// Recording audio format configuration bitfield.
    /// Key: `reccfg`
    pub record_config: Option<u32>,

    /// Recording general options bitfield.
    /// Key: `recopts`
    pub record_options: Option<u32>,

    /// Recording update frequency in ms.
    /// Key: `recupdatems`
    pub record_update_ms: Option<u32>,

    /// Show recording items during record.
    /// Key: `showrecitems`
    pub show_record_items: Option<bool>,

    /// Recording loop append behavior.
    /// Key: `recaddatloop`
    pub record_add_at_loop: Option<bool>,

    /// Recording filename wildcards.
    /// Key: `recfile_wildcards`
    pub record_file_wildcards: Option<String>,

    /// Prompt on end recording.
    /// Key: `promptendrec`
    pub prompt_end_record: Option<bool>,

    /// Maximum recording file size in MB.
    /// Key: `maxrecsize`
    pub max_record_size: Option<u32>,

    /// Use max recording size toggle.
    /// Key: `maxrecsize_use`
    pub max_record_size_use: Option<bool>,

    /// Disk check on record.
    /// Key: `diskcheck`
    pub disk_check: Option<bool>,

    /// Disk check MB threshold.
    /// Key: `diskcheckmb`
    pub disk_check_mb: Option<u32>,

    // ── Latency compensation ──
    /// Adjust recording latency.
    /// Key: `adjreclat`
    pub adjust_record_latency: Option<bool>,

    /// Adjust manual recording latency.
    /// Key: `adjrecmanlat`
    pub adjust_record_manual_latency: Option<bool>,

    /// Adjust manual latency input.
    /// Key: `adjrecmanlatin`
    pub adjust_record_manual_latency_input: Option<bool>,

    /// Manual latency offset in samples.
    /// Key: `manuallat`
    pub manual_latency: Option<i32>,

    /// Manual input latency in samples.
    /// Key: `manuallatin`
    pub manual_latency_input: Option<i32>,
}

impl Recording {
    pub(crate) fn from_ini(ini: &IniFile) -> Self {
        Self {
            record_config: get_u32(ini, S, "reccfg"),
            record_options: get_u32(ini, S, "recopts"),
            record_update_ms: get_u32(ini, S, "recupdatems"),
            show_record_items: get_bool(ini, S, "showrecitems"),
            record_add_at_loop: get_bool(ini, S, "recaddatloop"),
            record_file_wildcards: get_string(ini, S, "recfile_wildcards"),
            prompt_end_record: get_bool(ini, S, "promptendrec"),
            max_record_size: get_u32(ini, S, "maxrecsize"),
            max_record_size_use: get_bool(ini, S, "maxrecsize_use"),
            disk_check: get_bool(ini, S, "diskcheck"),
            disk_check_mb: get_u32(ini, S, "diskcheckmb"),
            adjust_record_latency: get_bool(ini, S, "adjreclat"),
            adjust_record_manual_latency: get_bool(ini, S, "adjrecmanlat"),
            adjust_record_manual_latency_input: get_bool(ini, S, "adjrecmanlatin"),
            manual_latency: get_i32(ini, S, "manuallat"),
            manual_latency_input: get_i32(ini, S, "manuallatin"),
        }
    }

    /// Returns a new [`Recording`] with REAPER factory default values.
    ///
    /// Fields whose factory value is an empty string are left as `None`.
    pub fn factory_defaults() -> Self {
        Self {
            record_config: None,             // reccfg= (empty)
            record_options: Some(0),         // recopts=0
            record_update_ms: Some(333),     // recupdatems=333
            show_record_items: Some(true),   // showrecitems=1
            record_add_at_loop: Some(false), // recaddatloop=0
            record_file_wildcards: Some(
                "$tracknumber-$track-$year2$month$day_$hour$minute".to_string(),
            ), // recfile_wildcards=...
            prompt_end_record: Some(false),  // promptendrec=0
            max_record_size: Some(1024),     // maxrecsize=1024
            max_record_size_use: Some(true), // maxrecsize_use=1
            disk_check: Some(true),          // diskcheck=1
            disk_check_mb: Some(1024),       // diskcheckmb=1024
            adjust_record_latency: Some(true), // adjreclat=1
            adjust_record_manual_latency: Some(false), // adjrecmanlat=0
            adjust_record_manual_latency_input: Some(false), // adjrecmanlatin=0
            manual_latency: Some(0),         // manuallat=0
            manual_latency_input: Some(0),   // manuallatin=0
        }
    }

    pub(crate) fn write_to(&self, ini: &mut IniFile) {
        set_opt_u32(ini, S, "reccfg", self.record_config);
        set_opt_u32(ini, S, "recopts", self.record_options);
        set_opt_u32(ini, S, "recupdatems", self.record_update_ms);
        set_opt_bool(ini, S, "showrecitems", self.show_record_items);
        set_opt_bool(ini, S, "recaddatloop", self.record_add_at_loop);
        set_opt_string(ini, S, "recfile_wildcards", &self.record_file_wildcards);
        set_opt_bool(ini, S, "promptendrec", self.prompt_end_record);
        set_opt_u32(ini, S, "maxrecsize", self.max_record_size);
        set_opt_bool(ini, S, "maxrecsize_use", self.max_record_size_use);
        set_opt_bool(ini, S, "diskcheck", self.disk_check);
        set_opt_u32(ini, S, "diskcheckmb", self.disk_check_mb);
        set_opt_bool(ini, S, "adjreclat", self.adjust_record_latency);
        set_opt_bool(ini, S, "adjrecmanlat", self.adjust_record_manual_latency);
        set_opt_bool(
            ini,
            S,
            "adjrecmanlatin",
            self.adjust_record_manual_latency_input,
        );
        set_opt_i32(ini, S, "manuallat", self.manual_latency);
        set_opt_i32(ini, S, "manuallatin", self.manual_latency_input);
    }
}
