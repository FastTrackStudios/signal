//! Video preferences (Preferences > Video and `[reaper_video]` section).

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::ini::IniFile;
use crate::parse::*;

const S: &str = "REAPER";
const V: &str = "reaper_video";

/// Video preferences (spans both `[REAPER]` and `[reaper_video]` sections).
#[derive(Debug, Clone, Default, Serialize, Deserialize, Facet)]
pub struct Video {
    // ── [REAPER] section ──
    /// Video color space setting.
    /// Key: `video_colorspace`
    pub color_space: Option<u32>,

    /// Video decode priority.
    /// Key: `video_decprio`
    pub decode_priority: Option<u32>,

    /// Video default image length.
    /// Key: `video_defimglen`
    pub default_image_length: Option<f64>,

    /// Video playback delay in ms.
    /// Key: `video_delayms`
    pub delay_ms: Option<u32>,

    // ── [reaper_video] section ──
    /// Video FX processing mode.
    /// Key: `fx_mode` (section: reaper_video)
    pub fx_mode: Option<u32>,

    /// Video miscellaneous flags.
    /// Key: `misc_flags` (section: reaper_video)
    pub misc_flags: Option<u32>,

    /// Video playback caching.
    /// Key: `playback_cache` (section: reaper_video)
    pub playback_cache: Option<u32>,

    /// Video sample rate.
    /// Key: `smp` (section: reaper_video)
    pub sample_rate: Option<u32>,

    /// Video prefetch sources.
    /// Key: `vdprefetch_srcs` (section: reaper_video)
    pub prefetch_sources: Option<u32>,

    /// Video prefetch thread count.
    /// Key: `vdprefetch_threads` (section: reaper_video)
    pub prefetch_threads: Option<u32>,

    /// Video visibility toggle.
    /// Key: `visible` (section: reaper_video)
    pub visible: Option<u32>,
}

impl Video {
    pub(crate) fn from_ini(ini: &IniFile) -> Self {
        Self {
            color_space: get_u32(ini, S, "video_colorspace"),
            decode_priority: get_u32(ini, S, "video_decprio"),
            default_image_length: get_f64(ini, S, "video_defimglen"),
            delay_ms: get_u32(ini, S, "video_delayms"),
            fx_mode: get_u32(ini, V, "fx_mode"),
            misc_flags: get_u32(ini, V, "misc_flags"),
            playback_cache: get_u32(ini, V, "playback_cache"),
            sample_rate: get_u32(ini, V, "smp"),
            prefetch_sources: get_u32(ini, V, "vdprefetch_srcs"),
            prefetch_threads: get_u32(ini, V, "vdprefetch_threads"),
            visible: get_u32(ini, V, "visible"),
        }
    }

    /// Returns a new [`Video`] with REAPER factory default values.
    ///
    /// Fields whose factory value is an empty string are left as `None`.
    pub fn factory_defaults() -> Self {
        Self {
            color_space: Some(263171),          // video_colorspace=263171
            decode_priority: None,              // video_decprio=vlc ffmpeg (not u32)
            default_image_length: Some(1000.0), // video_defimglen=1000
            delay_ms: Some(0),                  // video_delayms=0
            fx_mode: None,                      // reaper_video->fx_mode= (empty)
            misc_flags: None,                   // reaper_video->misc_flags= (empty)
            playback_cache: None,               // reaper_video->playback_cache= (empty)
            sample_rate: None,                  // reaper_video->smp= (empty)
            prefetch_sources: None,             // reaper_video->vdprefetch_srcs= (empty)
            prefetch_threads: None,             // reaper_video->vdprefetch_threads= (empty)
            visible: None,                      // reaper_video->visible= (empty)
        }
    }

    pub(crate) fn write_to(&self, ini: &mut IniFile) {
        set_opt_u32(ini, S, "video_colorspace", self.color_space);
        set_opt_u32(ini, S, "video_decprio", self.decode_priority);
        set_opt_f64(ini, S, "video_defimglen", self.default_image_length);
        set_opt_u32(ini, S, "video_delayms", self.delay_ms);
        set_opt_u32(ini, V, "fx_mode", self.fx_mode);
        set_opt_u32(ini, V, "misc_flags", self.misc_flags);
        set_opt_u32(ini, V, "playback_cache", self.playback_cache);
        set_opt_u32(ini, V, "smp", self.sample_rate);
        set_opt_u32(ini, V, "vdprefetch_srcs", self.prefetch_sources);
        set_opt_u32(ini, V, "vdprefetch_threads", self.prefetch_threads);
        set_opt_u32(ini, V, "visible", self.visible);
    }
}
