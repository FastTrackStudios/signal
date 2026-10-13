//! MIDI preferences (Preferences > MIDI).

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::ini::IniFile;
use crate::parse::*;

const S: &str = "REAPER";

/// MIDI device and editor preferences.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Facet)]
pub struct Midi {
    // ── Devices ──
    /// MIDI input device configuration.
    /// Key: `midiins`
    pub midi_inputs: Option<String>,

    /// MIDI input control surface config.
    /// Key: `midiins_cs`
    pub midi_inputs_control_surface: Option<String>,

    /// MIDI output device configuration.
    /// Key: `midiouts`
    pub midi_outputs: Option<String>,

    /// MIDI output clock configuration.
    /// Key: `midiouts_clock`
    pub midi_outputs_clock: Option<String>,

    /// MIDI clock without SPP config.
    /// Key: `midiouts_clock_nospp`
    pub midi_outputs_clock_no_spp: Option<String>,

    /// MIDI output low-latency mode.
    /// Key: `midiouts_llmode`
    pub midi_outputs_low_latency: Option<String>,

    /// MIDI output no-reset configuration.
    /// Key: `midiouts_noreset`
    pub midi_outputs_no_reset: Option<String>,

    /// Restrict MIDI hardware output to one thread.
    /// Key: `midioutthread`
    pub midi_output_thread: Option<bool>,

    /// MIDI sending behavior options bitfield.
    /// Key: `midisendflags`
    pub midi_send_flags: Option<u32>,

    // ── Editor ──
    /// MIDI CC event density display.
    /// Key: `midiccdensity`
    pub midi_cc_density: Option<u32>,

    /// MIDI CC envelope display bitfield.
    /// Key: `midiccenv`
    pub midi_cc_envelope: Option<u32>,

    /// MIDI CC interpolation mode.
    /// Key: `midiccinterp`
    pub midi_cc_interpolation: Option<u32>,

    /// MIDI CC touch timeout in ms.
    /// Key: `midicctouchtimeout`
    pub midi_cc_touch_timeout: Option<u32>,

    /// MIDI default color mapping.
    /// Key: `mididefcolormap`
    pub midi_default_color_map: Option<String>,

    /// MIDI editor general options bitfield.
    /// Key: `midieditor`
    pub midi_editor: Option<u32>,

    /// MIDI velocity metering options bitfield.
    /// Key: `midivu`
    pub midi_vu: Option<u32>,

    /// MIDI octave offset value.
    /// Key: `midioctoffs`
    pub midi_octave_offset: Option<i32>,

    /// MIDI ticks per beat resolution.
    /// Key: `miditicksperbeat`
    pub midi_ticks_per_beat: Option<u32>,

    /// MIDI input wildcards.
    /// Key: `inprojmidi_wildcards`
    pub in_project_midi_wildcards: Option<String>,

    /// Ribbon controller settings.
    /// Key: `rbn`
    pub ribbon: Option<u32>,

    /// Trim MIDI on split toggle.
    /// Key: `trimmidionsplit`
    pub trim_midi_on_split: Option<u32>,

    // ── Score / notation ──
    /// MIDI input and CC editing options bitfield.
    /// Key: `scnotes`
    pub score_notes: Option<u32>,

    /// Notation view minimum note length.
    /// Key: `scoreminnotelen`
    pub score_min_note_length: Option<u32>,

    /// Score quantization setting.
    /// Key: `scorequant`
    pub score_quantize: Option<u32>,

    /// Scene name editing behavior.
    /// Key: `scnameedit`
    pub scene_name_edit: Option<u32>,

    /// Item notes dialog settings bitfield.
    /// Key: `itemnotes`
    pub item_notes: Option<u32>,
}

impl Midi {
    pub(crate) fn from_ini(ini: &IniFile) -> Self {
        Self {
            midi_inputs: get_string(ini, S, "midiins"),
            midi_inputs_control_surface: get_string(ini, S, "midiins_cs"),
            midi_outputs: get_string(ini, S, "midiouts"),
            midi_outputs_clock: get_string(ini, S, "midiouts_clock"),
            midi_outputs_clock_no_spp: get_string(ini, S, "midiouts_clock_nospp"),
            midi_outputs_low_latency: get_string(ini, S, "midiouts_llmode"),
            midi_outputs_no_reset: get_string(ini, S, "midiouts_noreset"),
            midi_output_thread: get_bool(ini, S, "midioutthread"),
            midi_send_flags: get_u32(ini, S, "midisendflags"),
            midi_cc_density: get_u32(ini, S, "midiccdensity"),
            midi_cc_envelope: get_u32(ini, S, "midiccenv"),
            midi_cc_interpolation: get_u32(ini, S, "midiccinterp"),
            midi_cc_touch_timeout: get_u32(ini, S, "midicctouchtimeout"),
            midi_default_color_map: get_string(ini, S, "mididefcolormap"),
            midi_editor: get_u32(ini, S, "midieditor"),
            midi_vu: get_u32(ini, S, "midivu"),
            midi_octave_offset: get_i32(ini, S, "midioctoffs"),
            midi_ticks_per_beat: get_u32(ini, S, "miditicksperbeat"),
            in_project_midi_wildcards: get_string(ini, S, "inprojmidi_wildcards"),
            ribbon: get_u32(ini, S, "rbn"),
            trim_midi_on_split: get_u32(ini, S, "trimmidionsplit"),
            score_notes: get_u32(ini, S, "scnotes"),
            score_min_note_length: get_u32(ini, S, "scoreminnotelen"),
            score_quantize: get_u32(ini, S, "scorequant"),
            scene_name_edit: get_u32(ini, S, "scnameedit"),
            item_notes: get_u32(ini, S, "itemnotes"),
        }
    }

    /// Returns a new [`Midi`] with REAPER factory default values.
    ///
    /// Fields whose factory value is an empty string are left as `None`.
    pub fn factory_defaults() -> Self {
        Self {
            midi_inputs: None,                 // midiins= (empty)
            midi_inputs_control_surface: None, // midiins_cs= (empty)
            midi_outputs: None,                // midiouts= (empty)
            midi_outputs_clock: None,          // midiouts_clock= (empty)
            midi_outputs_clock_no_spp: None,   // midiouts_clock_nospp= (empty)
            midi_outputs_low_latency: None,    // midiouts_llmode= (empty)
            midi_outputs_no_reset: None,       // midiouts_noreset= (empty)
            midi_output_thread: Some(false),   // midioutthread=0
            midi_send_flags: Some(0),          // midisendflags=0
            midi_cc_density: Some(32),         // midiccdensity=32
            midi_cc_envelope: Some(16),        // midiccenv=16
            midi_cc_interpolation: Some(32),   // midiccinterp=32
            midi_cc_touch_timeout: Some(1000), // midicctouchtimeout=1000
            midi_default_color_map: None,      // mididefcolormap= (empty)
            midi_editor: Some(198273),         // midieditor=198273
            midi_vu: Some(0),                  // midivu=0
            midi_octave_offset: Some(1),       // midioctoffs=1
            midi_ticks_per_beat: Some(960),    // miditicksperbeat=960
            in_project_midi_wildcards: None,   // inprojmidi_wildcards= (empty)
            ribbon: Some(0),                   // rbn=0
            trim_midi_on_split: Some(1),       // trimmidionsplit=1
            score_notes: None,                 // scnotes not in factory defaults
            score_min_note_length: None, // scoreminnotelen=0.00000000000000 (float, unparseable as u32)
            score_quantize: None,        // scorequant=0.00000000000000 (float, unparseable as u32)
            scene_name_edit: Some(0),    // scnameedit=0
            item_notes: Some(8),         // itemnotes=8
        }
    }

    pub(crate) fn write_to(&self, ini: &mut IniFile) {
        set_opt_string(ini, S, "midiins", &self.midi_inputs);
        set_opt_string(ini, S, "midiins_cs", &self.midi_inputs_control_surface);
        set_opt_string(ini, S, "midiouts", &self.midi_outputs);
        set_opt_string(ini, S, "midiouts_clock", &self.midi_outputs_clock);
        set_opt_string(
            ini,
            S,
            "midiouts_clock_nospp",
            &self.midi_outputs_clock_no_spp,
        );
        set_opt_string(ini, S, "midiouts_llmode", &self.midi_outputs_low_latency);
        set_opt_string(ini, S, "midiouts_noreset", &self.midi_outputs_no_reset);
        set_opt_bool(ini, S, "midioutthread", self.midi_output_thread);
        set_opt_u32(ini, S, "midisendflags", self.midi_send_flags);
        set_opt_u32(ini, S, "midiccdensity", self.midi_cc_density);
        set_opt_u32(ini, S, "midiccenv", self.midi_cc_envelope);
        set_opt_u32(ini, S, "midiccinterp", self.midi_cc_interpolation);
        set_opt_u32(ini, S, "midicctouchtimeout", self.midi_cc_touch_timeout);
        set_opt_string(ini, S, "mididefcolormap", &self.midi_default_color_map);
        set_opt_u32(ini, S, "midieditor", self.midi_editor);
        set_opt_u32(ini, S, "midivu", self.midi_vu);
        set_opt_i32(ini, S, "midioctoffs", self.midi_octave_offset);
        set_opt_u32(ini, S, "miditicksperbeat", self.midi_ticks_per_beat);
        set_opt_string(
            ini,
            S,
            "inprojmidi_wildcards",
            &self.in_project_midi_wildcards,
        );
        set_opt_u32(ini, S, "rbn", self.ribbon);
        set_opt_u32(ini, S, "trimmidionsplit", self.trim_midi_on_split);
        set_opt_u32(ini, S, "scnotes", self.score_notes);
        set_opt_u32(ini, S, "scoreminnotelen", self.score_min_note_length);
        set_opt_u32(ini, S, "scorequant", self.score_quantize);
        set_opt_u32(ini, S, "scnameedit", self.scene_name_edit);
        set_opt_u32(ini, S, "itemnotes", self.item_notes);
    }
}
