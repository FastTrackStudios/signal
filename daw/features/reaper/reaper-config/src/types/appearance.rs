//! Appearance preferences (Preferences > Appearance).

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::ini::IniFile;
use crate::parse::*;
use crate::types::common::{GroupDisplayMode, ItemVolumeHandlePosition, PanDisplayMode, ZLayer};

const S: &str = "REAPER";

/// Appearance and display preferences.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Facet)]
pub struct Appearance {
    // ── Track control panel ──
    /// TCP element alignment.
    /// Key: `tcpalign`
    pub tcp_align: Option<u32>,

    /// TCP tint color value.
    /// Key: `tinttcp`
    pub tint_tcp: Option<u32>,

    /// Track gap max pixels (visual spacer between tracks).
    /// Key: `trackgapmax`
    pub track_gap_max: Option<u32>,

    /// Pixels between adjacent track items.
    /// Key: `trackitemgap`
    pub track_item_gap: Option<u32>,

    // ── Peaks / waveforms ──
    /// Waveform peak display options bitfield.
    /// Key: `showpeaks`
    pub show_peaks: Option<u32>,

    /// Minimum waveform peak height.
    /// Key: `peaks_minheight`
    pub peaks_min_height: Option<u32>,

    /// Peak edge rendering.
    /// Key: `peaksedges`
    pub peaks_edges: Option<u32>,

    /// Sample edge rendering toggle.
    /// Key: `sampleedges`
    pub sample_edges: Option<bool>,

    /// MIDI peak display options.
    /// Key: `midipeaks`
    pub midi_peaks: Option<u32>,

    /// Alternative peak cache usage bitfield.
    /// Key: `altpeaks`
    pub alt_peaks: Option<u32>,

    /// Alternate peaks cache directory.
    /// Key: `altpeakspath`
    pub alt_peaks_path: Option<String>,

    /// Alt peaks option path list.
    /// Key: `altpeaksopathlist`
    pub alt_peaks_option_path_list: Option<String>,

    // ── Spectral peaks ──
    /// Spectral peaks opacity (0-255).
    /// Key: `specpeak_alpha`
    pub specpeak_alpha: Option<u32>,

    /// Spectral peaks variance level.
    /// Key: `specpeak_bv`
    pub specpeak_bv: Option<u32>,

    /// Spectral peaks fade to theme color.
    /// Key: `specpeak_ftp`
    pub specpeak_ftp: Option<bool>,

    /// Spectral peaks high-end frequency.
    /// Key: `specpeak_hueh`
    pub specpeak_hue_high: Option<f64>,

    /// Spectral peaks low-end frequency.
    /// Key: `specpeak_huel`
    pub specpeak_hue_low: Option<f64>,

    /// Spectral peaks low-frequency part.
    /// Key: `specpeak_lo`
    pub specpeak_lo: Option<f64>,

    /// Spectral peaks noise threshold.
    /// Key: `specpeak_na`
    pub specpeak_noise: Option<f64>,

    // ── Items ──
    /// Item label display options.
    /// Key: `labelitems2`
    pub label_items: Option<u32>,

    /// Toggle display of media cues in items.
    /// Key: `cueitems`
    pub cue_items: Option<bool>,

    /// Toggle item label visibility.
    /// Key: `itemtexthide`
    pub item_text_hide: Option<bool>,

    /// Item icon display settings bitfield.
    /// Key: `itemicons`
    pub item_icons: Option<u32>,

    /// Item icon minimum display height.
    /// Key: `itemicons_minheight`
    pub item_icons_min_height: Option<u32>,

    /// Item label minimum display height.
    /// Key: `itemlabel_minheight`
    pub item_label_min_height: Option<u32>,

    /// Item label hide height threshold.
    /// Key: `itemlabel_hideheight`
    pub item_label_hide_height: Option<u32>,

    /// Item volume control representation.
    /// Key: `itemvolmode`
    #[facet(opaque)]
    pub item_vol_mode: Option<ItemVolumeHandlePosition>,

    /// Maximum overlapping item lanes.
    /// Key: `maxitemlanes`
    pub max_item_lanes: Option<u32>,

    /// Minimum item lane height.
    /// Key: `itemlane_minheight`
    pub item_lane_min_height: Option<u32>,

    /// Item lower-half minimum height.
    /// Key: `itemlowerhalf_minheight`
    pub item_lower_half_min_height: Option<u32>,

    /// Selected item tint alpha (0.0–1.0).
    /// Key: `selitem_tintalpha`
    pub selected_item_tint_alpha: Option<f64>,

    /// Unselected item tint alpha (0.0–1.0).
    /// Key: `unselitem_tintalpha`
    pub unselected_item_tint_alpha: Option<f64>,

    // ── Grid / ruler ──
    /// Dotted grid line visibility.
    /// Key: `griddot`
    pub grid_dot: Option<bool>,

    /// Grid line Z-order (behind or in front of items).
    /// Key: `gridinbg`
    #[facet(opaque)]
    pub grid_in_background: Option<ZLayer>,

    /// Marker line Z-order.
    /// Key: `gridinbg2`
    #[facet(opaque)]
    pub marker_in_background: Option<ZLayer>,

    /// Ruler label spacing slider.
    /// Key: `rulerlabelspacing`
    pub ruler_label_spacing: Option<u32>,

    /// Ruler region/marker/tempo display bitfield.
    /// Key: `rulerlayout`
    pub ruler_layout: Option<u32>,

    /// Guide line editing visibility.
    /// Key: `guidelines2`
    pub guidelines: Option<bool>,

    /// Vertical arrange view grid division bitfield.
    /// Key: `vgrid`
    pub vertical_grid: Option<u32>,

    // ── Envelopes ──
    /// Envelope point size scaling.
    /// Key: `env_pt_scale`
    pub envelope_point_scale: Option<f64>,

    /// Non-selected envelope point scale.
    /// Key: `env_pt_scale2`
    pub envelope_point_scale_unselected: Option<f64>,

    /// Envelope lane display options bitfield.
    /// Key: `envlanes`
    pub envelope_lanes: Option<u32>,

    // ── Misc display ──
    /// Faster text rendering toggle.
    /// Key: `nativedrawtext`
    pub native_draw_text: Option<u32>,

    /// Native text rendering 2.
    /// Key: `nativedrawtext2`
    pub native_draw_text_2: Option<u32>,

    /// Playback cursor width in pixels.
    /// Key: `playcursormode`
    pub play_cursor_mode: Option<u32>,

    /// Solid edge on selections bitfield.
    /// Key: `timeseledge`
    pub time_selection_edge: Option<u32>,

    /// Relative edge drawing toggle.
    /// Key: `relativeedges`
    pub relative_edges: Option<bool>,

    /// Vertical text rendering direction.
    /// Key: `textflags`
    pub text_flags: Option<u32>,

    /// Custom menu and toolbar appearance bitfield.
    /// Key: `custommenu`
    pub custom_menu: Option<u32>,

    /// Show peak build status window.
    /// Key: `showpeaksbuild`
    pub show_peaks_build: Option<bool>,

    /// Track grouping indicator style.
    /// Key: `groupdispmode`
    #[facet(opaque)]
    pub group_display_mode: Option<GroupDisplayMode>,

    /// How pan values are shown on faders.
    /// Key: `pandispmode`
    #[facet(opaque)]
    pub pan_display_mode: Option<PanDisplayMode>,

    /// Loudness graph opacity.
    /// Key: `loudgraph_alpha`
    pub loudness_graph_alpha: Option<f64>,

    /// Loudness peak opacity.
    /// Key: `loudpeak_alpha`
    pub loudness_peak_alpha: Option<f64>,

    // ── Mixer ──
    /// Mixer display and folder options bitfield.
    /// Key: `mixerflag`
    pub mixer_flags: Option<u32>,

    /// Mixer track filtering options bitfield.
    /// Key: `mixeruiflag`
    pub mixer_ui_flags: Option<u32>,

    /// Show master track visibility.
    /// Key: `showmaintrack`
    pub show_main_track: Option<bool>,

    /// Mixer track activation scroll.
    /// Key: `showctinmix`
    pub show_current_track_in_mixer: Option<bool>,

    // ── VU meters ──
    /// VU meter scale type.
    /// Key: `grvuscale`
    pub vu_scale: Option<u32>,

    /// Disable all VU meters.
    /// Key: `nometers`
    pub no_meters: Option<bool>,

    /// Reset VU on playback.
    /// Key: `resetvuplay`
    pub reset_vu_on_play: Option<bool>,

    /// VU clip indicator sticky.
    /// Key: `vuclipstick`
    pub vu_clip_stick: Option<bool>,

    /// VU meter decay rate.
    /// Key: `vudecay`
    pub vu_decay: Option<u32>,

    /// VU maximum volume display.
    /// Key: `vumaxvol`
    pub vu_max_vol: Option<f64>,

    /// VU minimum volume display.
    /// Key: `vuminvol`
    pub vu_min_vol: Option<f64>,

    /// VU update frequency in Hz.
    /// Key: `vuupdfreq`
    pub vu_update_freq: Option<u32>,

    // ── Master VU ──
    /// Master VU RMS display gain.
    /// Key: `mvu_rmsgain`
    pub master_vu_rms_gain: Option<u32>,

    /// Master VU metering mode bitfield.
    /// Key: `mvu_rmsmode`
    pub master_vu_rms_mode: Option<u32>,

    /// Master VU display offset.
    /// Key: `mvu_rmsoffs2`
    pub master_vu_rms_offset: Option<u32>,

    /// Master VU red threshold.
    /// Key: `mvu_rmsred`
    pub master_vu_rms_red: Option<u32>,

    /// Master VU window size in milliseconds.
    /// Key: `mvu_rmssize`
    pub master_vu_rms_size: Option<u32>,

    // ── Slider ──
    /// Slider maximum value.
    /// Key: `slidermaxv`
    pub slider_max: Option<f64>,

    /// Slider minimum value.
    /// Key: `sliderminv`
    pub slider_min: Option<f64>,

    /// Slider hexadecimal display.
    /// Key: `slidershex`
    pub slider_hex: Option<u32>,

    // ── Fullscreen ──
    /// Fullscreen mode toggle.
    /// Key: `isfullscreen`
    pub is_fullscreen: Option<bool>,

    /// Fullscreen coordinates.
    /// Key: `fullscreenrectl`, `fullscreenrectt`, `fullscreenrectr`, `fullscreenrectb`
    pub fullscreen_left: Option<i32>,
    pub fullscreen_top: Option<i32>,
    pub fullscreen_right: Option<i32>,
    pub fullscreen_bottom: Option<i32>,
}

impl Appearance {
    pub(crate) fn from_ini(ini: &IniFile) -> Self {
        Self {
            tcp_align: get_u32(ini, S, "tcpalign"),
            tint_tcp: get_u32(ini, S, "tinttcp"),
            track_gap_max: get_u32(ini, S, "trackgapmax"),
            track_item_gap: get_u32(ini, S, "trackitemgap"),
            show_peaks: get_u32(ini, S, "showpeaks"),
            peaks_min_height: get_u32(ini, S, "peaks_minheight"),
            peaks_edges: get_u32(ini, S, "peaksedges"),
            sample_edges: get_bool(ini, S, "sampleedges"),
            midi_peaks: get_u32(ini, S, "midipeaks"),
            alt_peaks: get_u32(ini, S, "altpeaks"),
            alt_peaks_path: get_string(ini, S, "altpeakspath"),
            alt_peaks_option_path_list: get_string(ini, S, "altpeaksopathlist"),
            specpeak_alpha: get_u32(ini, S, "specpeak_alpha"),
            specpeak_bv: get_u32(ini, S, "specpeak_bv"),
            specpeak_ftp: get_bool(ini, S, "specpeak_ftp"),
            specpeak_hue_high: get_f64(ini, S, "specpeak_hueh"),
            specpeak_hue_low: get_f64(ini, S, "specpeak_huel"),
            specpeak_lo: get_f64(ini, S, "specpeak_lo"),
            specpeak_noise: get_f64(ini, S, "specpeak_na"),
            label_items: get_u32(ini, S, "labelitems2"),
            cue_items: get_bool(ini, S, "cueitems"),
            item_text_hide: get_bool(ini, S, "itemtexthide"),
            item_icons: get_u32(ini, S, "itemicons"),
            item_icons_min_height: get_u32(ini, S, "itemicons_minheight"),
            item_label_min_height: get_u32(ini, S, "itemlabel_minheight"),
            item_label_hide_height: get_u32(ini, S, "itemlabel_hideheight"),
            item_vol_mode: get_enum_u32(ini, S, "itemvolmode"),
            max_item_lanes: get_u32(ini, S, "maxitemlanes"),
            item_lane_min_height: get_u32(ini, S, "itemlane_minheight"),
            item_lower_half_min_height: get_u32(ini, S, "itemlowerhalf_minheight"),
            selected_item_tint_alpha: get_f64(ini, S, "selitem_tintalpha"),
            unselected_item_tint_alpha: get_f64(ini, S, "unselitem_tintalpha"),
            grid_dot: get_bool(ini, S, "griddot"),
            grid_in_background: get_enum_u32(ini, S, "gridinbg"),
            marker_in_background: get_enum_u32(ini, S, "gridinbg2"),
            ruler_label_spacing: get_u32(ini, S, "rulerlabelspacing"),
            ruler_layout: get_u32(ini, S, "rulerlayout"),
            guidelines: get_bool(ini, S, "guidelines2"),
            vertical_grid: get_u32(ini, S, "vgrid"),
            envelope_point_scale: get_f64(ini, S, "env_pt_scale"),
            envelope_point_scale_unselected: get_f64(ini, S, "env_pt_scale2"),
            envelope_lanes: get_u32(ini, S, "envlanes"),
            native_draw_text: get_u32(ini, S, "nativedrawtext"),
            native_draw_text_2: get_u32(ini, S, "nativedrawtext2"),
            play_cursor_mode: get_u32(ini, S, "playcursormode"),
            time_selection_edge: get_u32(ini, S, "timeseledge"),
            relative_edges: get_bool(ini, S, "relativeedges"),
            text_flags: get_u32(ini, S, "textflags"),
            custom_menu: get_u32(ini, S, "custommenu"),
            show_peaks_build: get_bool(ini, S, "showpeaksbuild"),
            group_display_mode: get_enum_u32(ini, S, "groupdispmode"),
            pan_display_mode: get_enum_u32(ini, S, "pandispmode"),
            loudness_graph_alpha: get_f64(ini, S, "loudgraph_alpha"),
            loudness_peak_alpha: get_f64(ini, S, "loudpeak_alpha"),
            mixer_flags: get_u32(ini, S, "mixerflag"),
            mixer_ui_flags: get_u32(ini, S, "mixeruiflag"),
            show_main_track: get_bool(ini, S, "showmaintrack"),
            show_current_track_in_mixer: get_bool(ini, S, "showctinmix"),
            vu_scale: get_u32(ini, S, "grvuscale"),
            no_meters: get_bool(ini, S, "nometers"),
            reset_vu_on_play: get_bool(ini, S, "resetvuplay"),
            vu_clip_stick: get_bool(ini, S, "vuclipstick"),
            vu_decay: get_u32(ini, S, "vudecay"),
            vu_max_vol: get_f64(ini, S, "vumaxvol"),
            vu_min_vol: get_f64(ini, S, "vuminvol"),
            vu_update_freq: get_u32(ini, S, "vuupdfreq"),
            master_vu_rms_gain: get_u32(ini, S, "mvu_rmsgain"),
            master_vu_rms_mode: get_u32(ini, S, "mvu_rmsmode"),
            master_vu_rms_offset: get_u32(ini, S, "mvu_rmsoffs2"),
            master_vu_rms_red: get_u32(ini, S, "mvu_rmsred"),
            master_vu_rms_size: get_u32(ini, S, "mvu_rmssize"),
            slider_max: get_f64(ini, S, "slidermaxv"),
            slider_min: get_f64(ini, S, "sliderminv"),
            slider_hex: get_u32(ini, S, "slidershex"),
            is_fullscreen: get_bool(ini, S, "isfullscreen"),
            fullscreen_left: get_i32(ini, S, "fullscreenrectl"),
            fullscreen_top: get_i32(ini, S, "fullscreenrectt"),
            fullscreen_right: get_i32(ini, S, "fullscreenrectr"),
            fullscreen_bottom: get_i32(ini, S, "fullscreenrectb"),
        }
    }

    /// Returns a new [`Appearance`] with REAPER factory default values.
    ///
    /// Fields whose factory value is an empty string are left as `None`.
    pub fn factory_defaults() -> Self {
        Self {
            tcp_align: Some(771),                                 // tcpalign=771
            tint_tcp: Some(1537),                                 // tinttcp=1537
            track_gap_max: Some(16),                              // trackgapmax=16
            track_item_gap: Some(4),                              // trackitemgap=4
            show_peaks: Some(3),                                  // showpeaks=3
            peaks_min_height: Some(8),                            // peaks_minheight=8
            peaks_edges: Some(7),                                 // peaksedges=7
            sample_edges: Some(true),                             // sampleedges=1
            midi_peaks: Some(0),                                  // midipeaks=0
            alt_peaks: Some(4),                                   // altpeaks=4
            alt_peaks_path: None,                                 // altpeakspath= (empty)
            alt_peaks_option_path_list: None,                     // altpeaksopathlist= (empty)
            specpeak_alpha: Some(192),                            // specpeak_alpha=192
            specpeak_bv: Some(96),                                // specpeak_bv=96
            specpeak_ftp: Some(true),                             // specpeak_ftp=1
            specpeak_hue_high: Some(1.585),                       // specpeak_hueh=1.58500000000000
            specpeak_hue_low: Some(0.513),                        // specpeak_huel=0.51300000000000
            specpeak_lo: Some(0.0),                               // specpeak_lo=0.00000000000000
            specpeak_noise: Some(4.0),                            // specpeak_na=4.00000000000000
            label_items: Some(15),                                // labelitems2=15
            cue_items: Some(true),                                // cueitems=1
            item_text_hide: Some(false),                          // itemtexthide=0
            item_icons: Some(16765),                              // itemicons=16765
            item_icons_min_height: Some(36),                      // itemicons_minheight=36
            item_label_min_height: Some(28),                      // itemlabel_minheight=28
            item_label_hide_height: Some(28),                     // itemlabel_hideheight=28
            item_vol_mode: Some(ItemVolumeHandlePosition::AtTop), // itemvolmode=0
            max_item_lanes: None,                                 // maxitemlanes= (empty)
            item_lane_min_height: None,                           // itemlane_minheight= (empty)
            item_lower_half_min_height: Some(44),                 // itemlowerhalf_minheight=44
            selected_item_tint_alpha: Some(2.0),                  // selitem_tintalpha=2
            unselected_item_tint_alpha: Some(2.0),                // unselitem_tintalpha=2
            grid_dot: Some(true),                                 // griddot=1
            grid_in_background: Some(ZLayer::ThroughItems),       // gridinbg=1
            marker_in_background: Some(ZLayer::OverItems),        // gridinbg2=0
            ruler_label_spacing: Some(50),                        // rulerlabelspacing=50
            ruler_layout: Some(0),                                // rulerlayout=0
            guidelines: Some(true),                               // guidelines2=1
            vertical_grid: Some(0),                               // vgrid=0
            envelope_point_scale: Some(1.0),                      // env_pt_scale=1.00000000000000
            envelope_point_scale_unselected: Some(1.0),           // env_pt_scale2=1.00000000000000
            envelope_lanes: Some(23),                             // envlanes=23
            native_draw_text: None,                               // nativedrawtext= (empty)
            native_draw_text_2: None, // nativedrawtext2 not in factory defaults
            play_cursor_mode: Some(2), // playcursormode=2
            time_selection_edge: Some(0), // timeseledge=0
            relative_edges: Some(true), // relativeedges=1
            text_flags: Some(0),      // textflags=0
            custom_menu: Some(5),     // custommenu=5
            show_peaks_build: Some(true), // showpeaksbuild=1
            group_display_mode: Some(GroupDisplayMode::Ribbons), // groupdispmode=0
            pan_display_mode: Some(PanDisplayMode::FullRange), // pandispmode=0
            loudness_graph_alpha: Some(255.0), // loudgraph_alpha=255
            loudness_peak_alpha: Some(192.0), // loudpeak_alpha=192
            mixer_flags: Some(0),     // mixerflag=0
            mixer_ui_flags: None,     // mixeruiflag not in factory defaults
            show_main_track: Some(false), // showmaintrack=0
            show_current_track_in_mixer: Some(true), // showctinmix=1
            vu_scale: Some(2),        // grvuscale=2
            no_meters: Some(false),   // nometers=0
            reset_vu_on_play: Some(false), // resetvuplay=0
            vu_clip_stick: Some(true), // vuclipstick=1
            vu_decay: Some(120),      // vudecay=120
            vu_max_vol: Some(6.0),    // vumaxvol=6
            vu_min_vol: Some(-62.0),  // vuminvol=-62
            vu_update_freq: Some(30), // vuupdfreq=30
            master_vu_rms_gain: Some(0), // mvu_rmsgain=0
            master_vu_rms_mode: Some(1), // mvu_rmsmode=1
            master_vu_rms_offset: Some(140), // mvu_rmsoffs2=140
            master_vu_rms_red: Some(40), // mvu_rmsred=40
            master_vu_rms_size: Some(300), // mvu_rmssize=300
            slider_max: Some(12.0),   // slidermaxv=12.00000000000000
            slider_min: Some(-72.0),  // sliderminv=-72.00000000000000
            slider_hex: None,         // slidershex=-1.00000000000000 (negative, unparseable as u32)
            is_fullscreen: Some(false), // isfullscreen=0
            fullscreen_left: Some(0), // fullscreenrectl=0
            fullscreen_top: Some(0),  // fullscreenrectt=0
            fullscreen_right: Some(0), // fullscreenrectr=0
            fullscreen_bottom: Some(0), // fullscreenrectb=0
        }
    }

    pub(crate) fn write_to(&self, ini: &mut IniFile) {
        set_opt_u32(ini, S, "tcpalign", self.tcp_align);
        set_opt_u32(ini, S, "tinttcp", self.tint_tcp);
        set_opt_u32(ini, S, "trackgapmax", self.track_gap_max);
        set_opt_u32(ini, S, "trackitemgap", self.track_item_gap);
        set_opt_u32(ini, S, "showpeaks", self.show_peaks);
        set_opt_u32(ini, S, "peaks_minheight", self.peaks_min_height);
        set_opt_u32(ini, S, "peaksedges", self.peaks_edges);
        set_opt_bool(ini, S, "sampleedges", self.sample_edges);
        set_opt_u32(ini, S, "midipeaks", self.midi_peaks);
        set_opt_u32(ini, S, "altpeaks", self.alt_peaks);
        set_opt_string(ini, S, "altpeakspath", &self.alt_peaks_path);
        set_opt_string(
            ini,
            S,
            "altpeaksopathlist",
            &self.alt_peaks_option_path_list,
        );
        set_opt_u32(ini, S, "specpeak_alpha", self.specpeak_alpha);
        set_opt_u32(ini, S, "specpeak_bv", self.specpeak_bv);
        set_opt_bool(ini, S, "specpeak_ftp", self.specpeak_ftp);
        set_opt_f64(ini, S, "specpeak_hueh", self.specpeak_hue_high);
        set_opt_f64(ini, S, "specpeak_huel", self.specpeak_hue_low);
        set_opt_f64(ini, S, "specpeak_lo", self.specpeak_lo);
        set_opt_f64(ini, S, "specpeak_na", self.specpeak_noise);
        set_opt_u32(ini, S, "labelitems2", self.label_items);
        set_opt_bool(ini, S, "cueitems", self.cue_items);
        set_opt_bool(ini, S, "itemtexthide", self.item_text_hide);
        set_opt_u32(ini, S, "itemicons", self.item_icons);
        set_opt_u32(ini, S, "itemicons_minheight", self.item_icons_min_height);
        set_opt_u32(ini, S, "itemlabel_minheight", self.item_label_min_height);
        set_opt_u32(ini, S, "itemlabel_hideheight", self.item_label_hide_height);
        set_opt_enum_u32(ini, S, "itemvolmode", self.item_vol_mode);
        set_opt_u32(ini, S, "maxitemlanes", self.max_item_lanes);
        set_opt_u32(ini, S, "itemlane_minheight", self.item_lane_min_height);
        set_opt_u32(
            ini,
            S,
            "itemlowerhalf_minheight",
            self.item_lower_half_min_height,
        );
        set_opt_f64(ini, S, "selitem_tintalpha", self.selected_item_tint_alpha);
        set_opt_f64(
            ini,
            S,
            "unselitem_tintalpha",
            self.unselected_item_tint_alpha,
        );
        set_opt_bool(ini, S, "griddot", self.grid_dot);
        set_opt_enum_u32(ini, S, "gridinbg", self.grid_in_background);
        set_opt_enum_u32(ini, S, "gridinbg2", self.marker_in_background);
        set_opt_u32(ini, S, "rulerlabelspacing", self.ruler_label_spacing);
        set_opt_u32(ini, S, "rulerlayout", self.ruler_layout);
        set_opt_bool(ini, S, "guidelines2", self.guidelines);
        set_opt_u32(ini, S, "vgrid", self.vertical_grid);
        set_opt_f64(ini, S, "env_pt_scale", self.envelope_point_scale);
        set_opt_f64(
            ini,
            S,
            "env_pt_scale2",
            self.envelope_point_scale_unselected,
        );
        set_opt_u32(ini, S, "envlanes", self.envelope_lanes);
        set_opt_u32(ini, S, "nativedrawtext", self.native_draw_text);
        set_opt_u32(ini, S, "nativedrawtext2", self.native_draw_text_2);
        set_opt_u32(ini, S, "playcursormode", self.play_cursor_mode);
        set_opt_u32(ini, S, "timeseledge", self.time_selection_edge);
        set_opt_bool(ini, S, "relativeedges", self.relative_edges);
        set_opt_u32(ini, S, "textflags", self.text_flags);
        set_opt_u32(ini, S, "custommenu", self.custom_menu);
        set_opt_bool(ini, S, "showpeaksbuild", self.show_peaks_build);
        set_opt_enum_u32(ini, S, "groupdispmode", self.group_display_mode);
        set_opt_enum_u32(ini, S, "pandispmode", self.pan_display_mode);
        set_opt_f64(ini, S, "loudgraph_alpha", self.loudness_graph_alpha);
        set_opt_f64(ini, S, "loudpeak_alpha", self.loudness_peak_alpha);
        set_opt_u32(ini, S, "mixerflag", self.mixer_flags);
        set_opt_u32(ini, S, "mixeruiflag", self.mixer_ui_flags);
        set_opt_bool(ini, S, "showmaintrack", self.show_main_track);
        set_opt_bool(ini, S, "showctinmix", self.show_current_track_in_mixer);
        set_opt_u32(ini, S, "grvuscale", self.vu_scale);
        set_opt_bool(ini, S, "nometers", self.no_meters);
        set_opt_bool(ini, S, "resetvuplay", self.reset_vu_on_play);
        set_opt_bool(ini, S, "vuclipstick", self.vu_clip_stick);
        set_opt_u32(ini, S, "vudecay", self.vu_decay);
        set_opt_f64(ini, S, "vumaxvol", self.vu_max_vol);
        set_opt_f64(ini, S, "vuminvol", self.vu_min_vol);
        set_opt_u32(ini, S, "vuupdfreq", self.vu_update_freq);
        set_opt_u32(ini, S, "mvu_rmsgain", self.master_vu_rms_gain);
        set_opt_u32(ini, S, "mvu_rmsmode", self.master_vu_rms_mode);
        set_opt_u32(ini, S, "mvu_rmsoffs2", self.master_vu_rms_offset);
        set_opt_u32(ini, S, "mvu_rmsred", self.master_vu_rms_red);
        set_opt_u32(ini, S, "mvu_rmssize", self.master_vu_rms_size);
        set_opt_f64(ini, S, "slidermaxv", self.slider_max);
        set_opt_f64(ini, S, "sliderminv", self.slider_min);
        set_opt_u32(ini, S, "slidershex", self.slider_hex);
        set_opt_bool(ini, S, "isfullscreen", self.is_fullscreen);
        set_opt_i32(ini, S, "fullscreenrectl", self.fullscreen_left);
        set_opt_i32(ini, S, "fullscreenrectt", self.fullscreen_top);
        set_opt_i32(ini, S, "fullscreenrectr", self.fullscreen_right);
        set_opt_i32(ini, S, "fullscreenrectb", self.fullscreen_bottom);
    }
}
