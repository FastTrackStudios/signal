//! Editing preferences (Preferences > Editing).

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::ini::IniFile;
use crate::parse::*;
use crate::types::common::FadeCurveType;

const S: &str = "REAPER";

/// Editing behavior preferences.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Facet)]
pub struct Editing {
    // ── Mouse / cursor ──
    /// Item click cursor movement options bitfield.
    /// Key: `itemclickmovecurs`
    pub item_click_move_cursor: Option<u32>,

    /// Mouse wheel zoom direction.
    /// Key: `mousewheelmode`
    pub mouse_wheel_mode: Option<u32>,

    /// Mouse move modifier.
    /// Key: `mousemovemod`
    pub mouse_move_mod: Option<u32>,

    /// Track selection on mouse over.
    /// Key: `trackselonmouse`
    pub track_select_on_mouse: Option<u32>,

    /// Reverse horizontal zoom behavior when hand-scrolling.
    /// Key: `handzoom`
    pub hand_zoom: Option<bool>,

    /// Loop region click behavior.
    /// Key: `loopclickmode`
    pub loop_click_mode: Option<u32>,

    // ── Snapping ──
    /// Item snapping behavior.
    /// Key: `itemsnap`
    pub item_snap: Option<u32>,

    /// Snap relative to grid.
    /// Key: `relsnap`
    pub relative_snap: Option<bool>,

    /// Maximum snap track distance.
    /// Key: `maxsnaptrack`
    pub max_snap_track: Option<u32>,

    /// Snap extra radius value.
    /// Key: `snapextrad`
    pub snap_extra_radius: Option<f64>,

    /// Snap extra radius denominator.
    /// Key: `snapextraden`
    pub snap_extra_radius_denom: Option<u32>,

    // ── Fades ──
    /// Default fade length in seconds.
    /// Key: `deffadelen`
    pub default_fade_length: Option<f64>,

    /// Default fade curve shape.
    /// Key: `deffadeshape`
    #[facet(opaque)]
    pub default_fade_shape: Option<FadeCurveType>,

    /// Split crossfade default length.
    /// Key: `defsplitxfadelen`
    pub default_split_crossfade_length: Option<f64>,

    /// Default crossfade curve shape.
    /// Key: `defxfadeshape`
    #[facet(opaque)]
    pub default_crossfade_shape: Option<FadeCurveType>,

    /// Auto-crossfade on split toggle.
    /// Key: `splitautoxfade`
    pub split_auto_crossfade: Option<u32>,

    /// Maximum split point pixels.
    /// Key: `splitmaxpix`
    pub split_max_pixels: Option<u32>,

    /// Stretch marker fade behavior bitfield.
    /// Key: `stretchmarkerfade`
    pub stretch_marker_fade: Option<u32>,

    /// Fade handle minimum height.
    /// Key: `itemfade_minheight`
    pub item_fade_min_height: Option<u32>,

    /// Fade handle minimum width.
    /// Key: `itemfade_minwidth`
    pub item_fade_min_width: Option<u32>,

    /// Fade handle maximum width.
    /// Key: `itemfadehandle_maxwidth`
    pub item_fade_handle_max_width: Option<u32>,

    /// Fade edit display options bitfield.
    /// Key: `fadeeditflags`
    pub fade_edit_flags: Option<u32>,

    /// Fade edit link mode.
    /// Key: `fadeeditlink`
    pub fade_edit_link: Option<u32>,

    /// Fade edit post-selection.
    /// Key: `fadeeditpostsel`
    pub fade_edit_post_selection: Option<f64>,

    /// Fade edit pre-selection.
    /// Key: `fadeeditpresel`
    pub fade_edit_pre_selection: Option<f64>,

    // ── Ripple editing ──
    /// Ripple editing lock options bitfield.
    /// Key: `ripplelockmode`
    pub ripple_lock_mode: Option<u32>,

    /// Razor edit stretching behavior.
    /// Key: `areasel`
    pub area_selection: Option<u32>,

    // ── Item behavior ──
    /// Apply FX tail length in ms.
    /// Key: `applyfxtail`
    pub apply_fx_tail: Option<u32>,

    /// Item FX tail length settings bitfield.
    /// Key: `itemfxtail`
    pub item_fx_tail: Option<u32>,

    /// Loop newly created items.
    /// Key: `loopnewitems`
    pub loop_new_items: Option<u32>,

    /// Item double-click action.
    /// Key: `itemdblclk`
    pub item_double_click: Option<u32>,

    /// Item properties dialog.
    /// Key: `itemprops`
    pub item_properties: Option<u32>,

    /// Item time mode properties.
    /// Key: `itemprops_timemode`
    pub item_properties_time_mode: Option<u32>,

    /// Item edit precision.
    /// Key: `itemeditpr`
    pub item_edit_precision: Option<u32>,

    /// Take marker ranking behavior bitfield.
    /// Key: `itemranks`
    pub item_ranks: Option<u32>,

    /// Ctrl copy item behavior.
    /// Key: `ctrlcopyitem`
    pub ctrl_copy_item: Option<u32>,

    /// Select item to top.
    /// Key: `selitemtop`
    pub select_item_top: Option<u32>,

    // ── Transient detection ──
    /// Transient detection sensitivity.
    /// Key: `transientsensitivity`
    pub transient_sensitivity: Option<u32>,

    /// Transient detection threshold.
    /// Key: `transientthreshold`
    pub transient_threshold: Option<f64>,

    // ── Quantize ──
    /// Quantize item position options bitfield.
    /// Key: `quantflag`
    pub quantize_flags: Option<u32>,

    /// Item overlap extend duration in ms.
    /// Key: `quantolms`
    pub quantize_overlap_ms: Option<u32>,

    /// Item shorten threshold in ms.
    /// Key: `quantolms2`
    pub quantize_shorten_ms: Option<u32>,

    /// Quantize grid value.
    /// Key: `quantsize2`
    pub quantize_size: Option<f64>,

    // ── Seeking / playback ──
    /// Seeking behavior options bitfield.
    /// Key: `seekmodes`
    pub seek_modes: Option<u32>,

    /// Smooth seeking mode.
    /// Key: `smoothseek`
    pub smooth_seek: Option<u32>,

    /// Smooth seek measure count.
    /// Key: `smoothseekmeas`
    pub smooth_seek_measures: Option<u32>,

    /// Loop selection precision.
    /// Key: `loopselpr`
    pub loop_selection_precision: Option<u32>,

    /// Flush FX when looping.
    /// Key: `loopstopfx`
    pub loop_stop_fx: Option<bool>,

    /// Stop playback at end of loop if repeat is disabled.
    /// Key: `stopendofloop`
    pub stop_end_of_loop: Option<bool>,

    /// Stop/repeat playback at end of project.
    /// Key: `stopprojlen`
    pub stop_project_length: Option<bool>,

    /// View advancement mode.
    /// Key: `viewadvance`
    pub view_advance: Option<u32>,

    /// Maximum playback offset rate.
    /// Key: `maxplayoffsrate`
    pub max_playback_offset_rate: Option<u32>,

    /// Run actions after stop.
    /// Key: `runafterstop`
    pub run_after_stop: Option<u32>,

    /// Run all actions on stop.
    /// Key: `runallonstop`
    pub run_all_on_stop: Option<u32>,

    // ── Zoom ──
    /// Horizontal zoom mode.
    /// Key: `zoommode`
    pub zoom_mode: Option<u32>,

    /// Vertical zoom mode.
    /// Key: `vzoommode`
    pub vertical_zoom_mode: Option<u32>,

    /// Maximum vertical zoom level.
    /// Key: `maxvzoom`
    pub max_vertical_zoom: Option<f64>,

    /// Envelope vertical zoom scale.
    /// Key: `envvzoomscale`
    pub envelope_vertical_zoom_scale: Option<f64>,

    /// Item overlap offset percentage.
    /// Key: `itemoverlap_offspct`
    pub item_overlap_offset_percent: Option<f64>,

    /// Vertical scroll behavior bitfield.
    /// Key: `vscrollflag`
    pub vertical_scroll_flags: Option<u32>,

    /// Vertical scroll step size.
    /// Key: `vscrollstep`
    pub vertical_scroll_step: Option<u32>,

    /// Secondary scroll step.
    /// Key: `vscrollstep2`
    pub vertical_scroll_step_2: Option<u32>,

    /// Always show full track control panel on armed tracks.
    /// Key: `zoomshowarm`
    pub zoom_show_armed: Option<bool>,

    // ── Keyboard ──
    /// Tab-to-transport key settings bitfield.
    /// Key: `tabtotransflag`
    pub tab_to_transport_flags: Option<u32>,

    /// Keyboard override duration in ms.
    /// Key: `kbd_override_len`
    pub keyboard_override_length: Option<u32>,

    /// Use Alt key for modifiers.
    /// Key: `kbd_usealt`
    pub keyboard_use_alt: Option<u32>,

    // ── Multitouch ──
    /// Multitouch gesture support.
    /// Key: `multitouch`
    pub multitouch: Option<u32>,

    /// Multitouch ignore time in ms.
    /// Key: `multitouch_ignore_ms`
    pub multitouch_ignore_ms: Option<u32>,

    /// Multitouch wheel ignore time in ms.
    /// Key: `multitouch_ignorewheel_ms`
    pub multitouch_ignore_wheel_ms: Option<u32>,

    /// Multitouch rotation sensitivity.
    /// Key: `multitouch_rotate_gear`
    pub multitouch_rotate_gear: Option<f64>,

    /// Multitouch swipe sensitivity.
    /// Key: `multitouch_swipe_gear`
    pub multitouch_swipe_gear: Option<f64>,

    /// Multitouch zoom sensitivity.
    /// Key: `multitouch_zoom_gear`
    pub multitouch_zoom_gear: Option<f64>,

    // ── Scrub ──
    /// Scrubbing playback mode.
    /// Key: `scrubmode`
    pub scrub_mode: Option<u32>,

    /// Scrub playback gain in dB.
    /// Key: `scrubgain`
    pub scrub_gain: Option<f64>,

    /// Scrub relative gain in dB.
    /// Key: `scrubrelgain`
    pub scrub_relative_gain: Option<f64>,

    /// Scrub loop start point.
    /// Key: `scrubloopstart`
    pub scrub_loop_start: Option<u32>,

    /// Scrub loop end point.
    /// Key: `scrubloopend`
    pub scrub_loop_end: Option<u32>,

    // ── Transport ──
    /// Transport control flags bitfield.
    /// Key: `transflags`
    pub transport_flags: Option<u32>,

    /// Link loop points to time selection.
    /// Key: `locklooptotime`
    pub lock_loop_to_time: Option<bool>,

    /// Pre-roll playback and recording settings bitfield.
    /// Key: `preroll`
    pub preroll: Option<u32>,

    /// Pre-roll measures count.
    /// Key: `prerollmeas`
    pub preroll_measures: Option<f64>,

    /// Screenset view storage options.
    /// Key: `screenset_as_views`
    pub screenset_as_views: Option<u32>,

    /// Screenset window storage options.
    /// Key: `screenset_as_win`
    pub screenset_as_windows: Option<u32>,

    /// Auto-save screenset when switching.
    /// Key: `screenset_autosave`
    pub screenset_auto_save: Option<bool>,
}

impl Editing {
    pub(crate) fn from_ini(ini: &IniFile) -> Self {
        Self {
            item_click_move_cursor: get_u32(ini, S, "itemclickmovecurs"),
            mouse_wheel_mode: get_u32(ini, S, "mousewheelmode"),
            mouse_move_mod: get_u32(ini, S, "mousemovemod"),
            track_select_on_mouse: get_u32(ini, S, "trackselonmouse"),
            hand_zoom: get_bool(ini, S, "handzoom"),
            loop_click_mode: get_u32(ini, S, "loopclickmode"),
            item_snap: get_u32(ini, S, "itemsnap"),
            relative_snap: get_bool(ini, S, "relsnap"),
            max_snap_track: get_u32(ini, S, "maxsnaptrack"),
            snap_extra_radius: get_f64(ini, S, "snapextrad"),
            snap_extra_radius_denom: get_u32(ini, S, "snapextraden"),
            default_fade_length: get_f64(ini, S, "deffadelen"),
            default_fade_shape: get_enum_u32(ini, S, "deffadeshape"),
            default_split_crossfade_length: get_f64(ini, S, "defsplitxfadelen"),
            default_crossfade_shape: get_enum_u32(ini, S, "defxfadeshape"),
            split_auto_crossfade: get_u32(ini, S, "splitautoxfade"),
            split_max_pixels: get_u32(ini, S, "splitmaxpix"),
            stretch_marker_fade: get_u32(ini, S, "stretchmarkerfade"),
            item_fade_min_height: get_u32(ini, S, "itemfade_minheight"),
            item_fade_min_width: get_u32(ini, S, "itemfade_minwidth"),
            item_fade_handle_max_width: get_u32(ini, S, "itemfadehandle_maxwidth"),
            fade_edit_flags: get_u32(ini, S, "fadeeditflags"),
            fade_edit_link: get_u32(ini, S, "fadeeditlink"),
            fade_edit_post_selection: get_f64(ini, S, "fadeeditpostsel"),
            fade_edit_pre_selection: get_f64(ini, S, "fadeeditpresel"),
            ripple_lock_mode: get_u32(ini, S, "ripplelockmode"),
            area_selection: get_u32(ini, S, "areasel"),
            apply_fx_tail: get_u32(ini, S, "applyfxtail"),
            item_fx_tail: get_u32(ini, S, "itemfxtail"),
            loop_new_items: get_u32(ini, S, "loopnewitems"),
            item_double_click: get_u32(ini, S, "itemdblclk"),
            item_properties: get_u32(ini, S, "itemprops"),
            item_properties_time_mode: get_u32(ini, S, "itemprops_timemode"),
            item_edit_precision: get_u32(ini, S, "itemeditpr"),
            item_ranks: get_u32(ini, S, "itemranks"),
            ctrl_copy_item: get_u32(ini, S, "ctrlcopyitem"),
            select_item_top: get_u32(ini, S, "selitemtop"),
            transient_sensitivity: get_u32(ini, S, "transientsensitivity"),
            transient_threshold: get_f64(ini, S, "transientthreshold"),
            quantize_flags: get_u32(ini, S, "quantflag"),
            quantize_overlap_ms: get_u32(ini, S, "quantolms"),
            quantize_shorten_ms: get_u32(ini, S, "quantolms2"),
            quantize_size: get_f64(ini, S, "quantsize2"),
            seek_modes: get_u32(ini, S, "seekmodes"),
            smooth_seek: get_u32(ini, S, "smoothseek"),
            smooth_seek_measures: get_u32(ini, S, "smoothseekmeas"),
            loop_selection_precision: get_u32(ini, S, "loopselpr"),
            loop_stop_fx: get_bool(ini, S, "loopstopfx"),
            stop_end_of_loop: get_bool(ini, S, "stopendofloop"),
            stop_project_length: get_bool(ini, S, "stopprojlen"),
            view_advance: get_u32(ini, S, "viewadvance"),
            max_playback_offset_rate: get_u32(ini, S, "maxplayoffsrate"),
            run_after_stop: get_u32(ini, S, "runafterstop"),
            run_all_on_stop: get_u32(ini, S, "runallonstop"),
            zoom_mode: get_u32(ini, S, "zoommode"),
            vertical_zoom_mode: get_u32(ini, S, "vzoommode"),
            max_vertical_zoom: get_f64(ini, S, "maxvzoom"),
            envelope_vertical_zoom_scale: get_f64(ini, S, "envvzoomscale"),
            item_overlap_offset_percent: get_f64(ini, S, "itemoverlap_offspct"),
            vertical_scroll_flags: get_u32(ini, S, "vscrollflag"),
            vertical_scroll_step: get_u32(ini, S, "vscrollstep"),
            vertical_scroll_step_2: get_u32(ini, S, "vscrollstep2"),
            zoom_show_armed: get_bool(ini, S, "zoomshowarm"),
            tab_to_transport_flags: get_u32(ini, S, "tabtotransflag"),
            keyboard_override_length: get_u32(ini, S, "kbd_override_len"),
            keyboard_use_alt: get_u32(ini, S, "kbd_usealt"),
            multitouch: get_u32(ini, S, "multitouch"),
            multitouch_ignore_ms: get_u32(ini, S, "multitouch_ignore_ms"),
            multitouch_ignore_wheel_ms: get_u32(ini, S, "multitouch_ignorewheel_ms"),
            multitouch_rotate_gear: get_f64(ini, S, "multitouch_rotate_gear"),
            multitouch_swipe_gear: get_f64(ini, S, "multitouch_swipe_gear"),
            multitouch_zoom_gear: get_f64(ini, S, "multitouch_zoom_gear"),
            scrub_mode: get_u32(ini, S, "scrubmode"),
            scrub_gain: get_f64(ini, S, "scrubgain"),
            scrub_relative_gain: get_f64(ini, S, "scrubrelgain"),
            scrub_loop_start: get_u32(ini, S, "scrubloopstart"),
            scrub_loop_end: get_u32(ini, S, "scrubloopend"),
            transport_flags: get_u32(ini, S, "transflags"),
            lock_loop_to_time: get_bool(ini, S, "locklooptotime"),
            preroll: get_u32(ini, S, "preroll"),
            preroll_measures: get_f64(ini, S, "prerollmeas"),
            screenset_as_views: get_u32(ini, S, "screenset_as_views"),
            screenset_as_windows: get_u32(ini, S, "screenset_as_win"),
            screenset_auto_save: get_bool(ini, S, "screenset_autosave"),
        }
    }

    /// Returns a new [`Editing`] with REAPER factory default values.
    ///
    /// Fields whose factory value is an empty string or cannot be parsed into
    /// the field type (e.g. a floating-point default for an `Option<u32>` field)
    /// are left as `None`.
    pub fn factory_defaults() -> Self {
        Self {
            item_click_move_cursor: Some(3),  // itemclickmovecurs=3
            mouse_wheel_mode: Some(2),        // mousewheelmode=2
            mouse_move_mod: None,             // mousemovemod not in factory defaults
            track_select_on_mouse: Some(1),   // trackselonmouse=1
            hand_zoom: Some(false),           // handzoom=0
            loop_click_mode: Some(0),         // loopclickmode=0
            item_snap: Some(4),               // itemsnap=4
            relative_snap: Some(false),       // relsnap=0
            max_snap_track: Some(0),          // maxsnaptrack=0
            snap_extra_radius: Some(2.0),     // snapextrad=2.00000000000000
            snap_extra_radius_denom: Some(0), // snapextraden=0
            default_fade_length: Some(0.01),  // deffadelen=0.01000000000000
            default_fade_shape: Some(FadeCurveType::Square), // deffadeshape=1
            default_split_crossfade_length: Some(0.01), // defsplitxfadelen=0.01000000000000
            default_crossfade_shape: Some(FadeCurveType::Unknown(7)), // defxfadeshape=7
            split_auto_crossfade: Some(32),   // splitautoxfade=32
            split_max_pixels: Some(50),       // splitmaxpix=50
            stretch_marker_fade: None, // stretchmarkerfade=2.50000000000000 (float, unparseable as u32)
            item_fade_min_height: Some(8), // itemfade_minheight=8
            item_fade_min_width: Some(28), // itemfade_minwidth=28
            item_fade_handle_max_width: Some(4), // itemfadehandle_maxwidth=4
            fade_edit_flags: None,     // fadeeditflags not in factory defaults
            fade_edit_link: None,      // fadeeditlink not in factory defaults
            fade_edit_post_selection: None, // fadeeditpostsel not in factory defaults
            fade_edit_pre_selection: None, // fadeeditpresel not in factory defaults
            ripple_lock_mode: Some(0), // ripplelockmode=0
            area_selection: Some(0),   // areasel=0
            apply_fx_tail: Some(1000), // applyfxtail=1000
            item_fx_tail: Some(2000),  // itemfxtail=2000
            loop_new_items: Some(14),  // loopnewitems=14
            item_double_click: None,   // itemdblclk not in factory defaults
            item_properties: None,     // itemprops not in factory defaults
            item_properties_time_mode: None, // itemprops_timemode not in factory defaults
            item_edit_precision: Some(1000), // itemeditpr=1000
            item_ranks: Some(19),      // itemranks=19
            ctrl_copy_item: None,      // ctrlcopyitem not in factory defaults
            select_item_top: None,     // selitemtop not in factory defaults
            transient_sensitivity: None, // transientsensitivity=0.50000000000000 (float, unparseable as u32)
            transient_threshold: Some(-24.0), // transientthreshold=-24.00000000000000
            quantize_flags: Some(0),     // quantflag=0
            quantize_overlap_ms: Some(30), // quantolms=30
            quantize_shorten_ms: Some(30), // quantolms2=30
            quantize_size: Some(0.25),   // quantsize2=0.25000000000000
            seek_modes: Some(65535),     // seekmodes=65535
            smooth_seek: Some(0),        // smoothseek=0
            smooth_seek_measures: Some(1), // smoothseekmeas=1
            loop_selection_precision: Some(1000), // loopselpr=1000
            loop_stop_fx: Some(false),   // loopstopfx=0
            stop_end_of_loop: Some(false), // stopendofloop=0
            stop_project_length: Some(true), // stopprojlen=1
            view_advance: Some(3),       // viewadvance=3
            max_playback_offset_rate: None, // maxplayoffsrate=2.00000000000000 (float, unparseable as u32)
            run_after_stop: Some(4000),     // runafterstop=4000
            run_all_on_stop: Some(1),       // runallonstop=1
            zoom_mode: Some(0),             // zoommode=0
            vertical_zoom_mode: Some(0),    // vzoommode=0
            max_vertical_zoom: Some(1.0),   // maxvzoom=1.00000000000000
            envelope_vertical_zoom_scale: Some(0.5), // envvzoomscale=0.50000000000000
            item_overlap_offset_percent: Some(100.0), // itemoverlap_offspct=100
            vertical_scroll_flags: Some(0), // vscrollflag=0
            vertical_scroll_step: None, // vscrollstep=0.50000000000000 (float, unparseable as u32)
            vertical_scroll_step_2: None, // vscrollstep2=0.10000000000000 (float, unparseable as u32)
            zoom_show_armed: Some(true),  // zoomshowarm=1
            tab_to_transport_flags: Some(0), // tabtotransflag=0
            keyboard_override_length: Some(1000), // kbd_override_len=1000
            keyboard_use_alt: Some(0),    // kbd_usealt=0
            multitouch: Some(7),          // multitouch=7
            multitouch_ignore_ms: Some(100), // multitouch_ignore_ms=100
            multitouch_ignore_wheel_ms: Some(100), // multitouch_ignorewheel_ms=100
            multitouch_rotate_gear: Some(1.0), // multitouch_rotate_gear=1.00000000000000
            multitouch_swipe_gear: Some(1.0), // multitouch_swipe_gear=1.00000000000000
            multitouch_zoom_gear: Some(1.0), // multitouch_zoom_gear=1.00000000000000
            scrub_mode: Some(1),          // scrubmode=1
            scrub_gain: Some(1.0),        // scrubgain=1.00000000000000
            scrub_relative_gain: Some(1.0), // scrubrelgain=1.00000000000000
            scrub_loop_start: None,       // scrubloopstart=-88 (negative, unparseable as u32)
            scrub_loop_end: Some(0),      // scrubloopend=0
            transport_flags: Some(0),     // transflags=0
            lock_loop_to_time: Some(true), // locklooptotime=1
            preroll: Some(0),             // preroll=0
            preroll_measures: Some(2.0),  // prerollmeas=2.00000000000000
            screenset_as_views: None,     // screenset_as_views=-1 (negative, unparseable as u32)
            screenset_as_windows: None,   // screenset_as_win=-1 (negative, unparseable as u32)
            screenset_auto_save: Some(false), // screenset_autosave=0
        }
    }

    pub(crate) fn write_to(&self, ini: &mut IniFile) {
        set_opt_u32(ini, S, "itemclickmovecurs", self.item_click_move_cursor);
        set_opt_u32(ini, S, "mousewheelmode", self.mouse_wheel_mode);
        set_opt_u32(ini, S, "mousemovemod", self.mouse_move_mod);
        set_opt_u32(ini, S, "trackselonmouse", self.track_select_on_mouse);
        set_opt_bool(ini, S, "handzoom", self.hand_zoom);
        set_opt_u32(ini, S, "loopclickmode", self.loop_click_mode);
        set_opt_u32(ini, S, "itemsnap", self.item_snap);
        set_opt_bool(ini, S, "relsnap", self.relative_snap);
        set_opt_u32(ini, S, "maxsnaptrack", self.max_snap_track);
        set_opt_f64(ini, S, "snapextrad", self.snap_extra_radius);
        set_opt_u32(ini, S, "snapextraden", self.snap_extra_radius_denom);
        set_opt_f64(ini, S, "deffadelen", self.default_fade_length);
        set_opt_enum_u32(ini, S, "deffadeshape", self.default_fade_shape);
        set_opt_f64(
            ini,
            S,
            "defsplitxfadelen",
            self.default_split_crossfade_length,
        );
        set_opt_enum_u32(ini, S, "defxfadeshape", self.default_crossfade_shape);
        set_opt_u32(ini, S, "splitautoxfade", self.split_auto_crossfade);
        set_opt_u32(ini, S, "splitmaxpix", self.split_max_pixels);
        set_opt_u32(ini, S, "stretchmarkerfade", self.stretch_marker_fade);
        set_opt_u32(ini, S, "itemfade_minheight", self.item_fade_min_height);
        set_opt_u32(ini, S, "itemfade_minwidth", self.item_fade_min_width);
        set_opt_u32(
            ini,
            S,
            "itemfadehandle_maxwidth",
            self.item_fade_handle_max_width,
        );
        set_opt_u32(ini, S, "fadeeditflags", self.fade_edit_flags);
        set_opt_u32(ini, S, "fadeeditlink", self.fade_edit_link);
        set_opt_f64(ini, S, "fadeeditpostsel", self.fade_edit_post_selection);
        set_opt_f64(ini, S, "fadeeditpresel", self.fade_edit_pre_selection);
        set_opt_u32(ini, S, "ripplelockmode", self.ripple_lock_mode);
        set_opt_u32(ini, S, "areasel", self.area_selection);
        set_opt_u32(ini, S, "applyfxtail", self.apply_fx_tail);
        set_opt_u32(ini, S, "itemfxtail", self.item_fx_tail);
        set_opt_u32(ini, S, "loopnewitems", self.loop_new_items);
        set_opt_u32(ini, S, "itemdblclk", self.item_double_click);
        set_opt_u32(ini, S, "itemprops", self.item_properties);
        set_opt_u32(ini, S, "itemprops_timemode", self.item_properties_time_mode);
        set_opt_u32(ini, S, "itemeditpr", self.item_edit_precision);
        set_opt_u32(ini, S, "itemranks", self.item_ranks);
        set_opt_u32(ini, S, "ctrlcopyitem", self.ctrl_copy_item);
        set_opt_u32(ini, S, "selitemtop", self.select_item_top);
        set_opt_u32(ini, S, "transientsensitivity", self.transient_sensitivity);
        set_opt_f64(ini, S, "transientthreshold", self.transient_threshold);
        set_opt_u32(ini, S, "quantflag", self.quantize_flags);
        set_opt_u32(ini, S, "quantolms", self.quantize_overlap_ms);
        set_opt_u32(ini, S, "quantolms2", self.quantize_shorten_ms);
        set_opt_f64(ini, S, "quantsize2", self.quantize_size);
        set_opt_u32(ini, S, "seekmodes", self.seek_modes);
        set_opt_u32(ini, S, "smoothseek", self.smooth_seek);
        set_opt_u32(ini, S, "smoothseekmeas", self.smooth_seek_measures);
        set_opt_u32(ini, S, "loopselpr", self.loop_selection_precision);
        set_opt_bool(ini, S, "loopstopfx", self.loop_stop_fx);
        set_opt_bool(ini, S, "stopendofloop", self.stop_end_of_loop);
        set_opt_bool(ini, S, "stopprojlen", self.stop_project_length);
        set_opt_u32(ini, S, "viewadvance", self.view_advance);
        set_opt_u32(ini, S, "maxplayoffsrate", self.max_playback_offset_rate);
        set_opt_u32(ini, S, "runafterstop", self.run_after_stop);
        set_opt_u32(ini, S, "runallonstop", self.run_all_on_stop);
        set_opt_u32(ini, S, "zoommode", self.zoom_mode);
        set_opt_u32(ini, S, "vzoommode", self.vertical_zoom_mode);
        set_opt_f64(ini, S, "maxvzoom", self.max_vertical_zoom);
        set_opt_f64(ini, S, "envvzoomscale", self.envelope_vertical_zoom_scale);
        set_opt_f64(
            ini,
            S,
            "itemoverlap_offspct",
            self.item_overlap_offset_percent,
        );
        set_opt_u32(ini, S, "vscrollflag", self.vertical_scroll_flags);
        set_opt_u32(ini, S, "vscrollstep", self.vertical_scroll_step);
        set_opt_u32(ini, S, "vscrollstep2", self.vertical_scroll_step_2);
        set_opt_bool(ini, S, "zoomshowarm", self.zoom_show_armed);
        set_opt_u32(ini, S, "tabtotransflag", self.tab_to_transport_flags);
        set_opt_u32(ini, S, "kbd_override_len", self.keyboard_override_length);
        set_opt_u32(ini, S, "kbd_usealt", self.keyboard_use_alt);
        set_opt_u32(ini, S, "multitouch", self.multitouch);
        set_opt_u32(ini, S, "multitouch_ignore_ms", self.multitouch_ignore_ms);
        set_opt_u32(
            ini,
            S,
            "multitouch_ignorewheel_ms",
            self.multitouch_ignore_wheel_ms,
        );
        set_opt_f64(
            ini,
            S,
            "multitouch_rotate_gear",
            self.multitouch_rotate_gear,
        );
        set_opt_f64(ini, S, "multitouch_swipe_gear", self.multitouch_swipe_gear);
        set_opt_f64(ini, S, "multitouch_zoom_gear", self.multitouch_zoom_gear);
        set_opt_u32(ini, S, "scrubmode", self.scrub_mode);
        set_opt_f64(ini, S, "scrubgain", self.scrub_gain);
        set_opt_f64(ini, S, "scrubrelgain", self.scrub_relative_gain);
        set_opt_u32(ini, S, "scrubloopstart", self.scrub_loop_start);
        set_opt_u32(ini, S, "scrubloopend", self.scrub_loop_end);
        set_opt_u32(ini, S, "transflags", self.transport_flags);
        set_opt_bool(ini, S, "locklooptotime", self.lock_loop_to_time);
        set_opt_u32(ini, S, "preroll", self.preroll);
        set_opt_f64(ini, S, "prerollmeas", self.preroll_measures);
        set_opt_u32(ini, S, "screenset_as_views", self.screenset_as_views);
        set_opt_u32(ini, S, "screenset_as_win", self.screenset_as_windows);
        set_opt_bool(ini, S, "screenset_autosave", self.screenset_auto_save);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ini::IniFile;
    use crate::types::common::FadeCurveType;

    fn ini_from(text: &str) -> IniFile {
        IniFile::parse(text)
    }

    #[test]
    fn parse_default_fade_shape() {
        let ini = ini_from("[REAPER]\ndeffadeshape=0\n");
        let e = Editing::from_ini(&ini);
        assert_eq!(e.default_fade_shape, Some(FadeCurveType::Linear));
    }

    #[test]
    fn parse_default_crossfade_shape() {
        let ini = ini_from("[REAPER]\ndefxfadeshape=5\n");
        let e = Editing::from_ini(&ini);
        assert_eq!(e.default_crossfade_shape, Some(FadeCurveType::Bezier));
    }

    #[test]
    fn parse_fade_curve_all_values() {
        for (raw, expected) in [
            (0u32, FadeCurveType::Linear),
            (1, FadeCurveType::Square),
            (2, FadeCurveType::SlowStartEnd),
            (3, FadeCurveType::FastStart),
            (4, FadeCurveType::FastEnd),
            (5, FadeCurveType::Bezier),
        ] {
            let ini = ini_from(&format!("[REAPER]\ndeffadeshape={raw}\n"));
            let e = Editing::from_ini(&ini);
            assert_eq!(e.default_fade_shape, Some(expected));
        }
    }

    #[test]
    fn parse_fade_curve_unknown() {
        let ini = ini_from("[REAPER]\ndeffadeshape=99\n");
        let e = Editing::from_ini(&ini);
        assert_eq!(e.default_fade_shape, Some(FadeCurveType::Unknown(99)));
    }

    #[test]
    fn round_trip_fade_shapes() {
        let ini = ini_from("[REAPER]\ndeffadeshape=3\ndefxfadeshape=2\n");
        let e = Editing::from_ini(&ini);
        assert_eq!(e.default_fade_shape, Some(FadeCurveType::FastStart));
        assert_eq!(e.default_crossfade_shape, Some(FadeCurveType::SlowStartEnd));

        let mut out = IniFile::parse("[REAPER]\n");
        e.write_to(&mut out);
        assert_eq!(out.get("REAPER", "deffadeshape"), Some("3"));
        assert_eq!(out.get("REAPER", "defxfadeshape"), Some("2"));
    }

    #[test]
    fn absent_fade_shapes_are_none() {
        let ini = ini_from("[REAPER]\n");
        let e = Editing::from_ini(&ini);
        assert!(e.default_fade_shape.is_none());
        assert!(e.default_crossfade_shape.is_none());
    }
}
