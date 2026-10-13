//! Rendering preferences (Preferences > Rendering).

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::ini::IniFile;
use crate::parse::*;

const S: &str = "REAPER";

/// Rendering / bounce preferences.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Facet)]
pub struct Rendering {
    /// Render configuration flags bitfield.
    /// Key: `rendercfg`
    pub render_config: Option<u32>,

    /// Bounce/render configuration bitfield.
    /// Key: `bouncecfg`
    pub bounce_config: Option<u32>,

    /// Render metadata handling bitfield.
    /// Key: `rendermetadataflags`
    pub render_metadata_flags: Option<u32>,

    /// Render peak generation.
    /// Key: `renderpeaks`
    pub render_peaks: Option<u32>,

    /// Render queue delay.
    /// Key: `renderqdelay`
    pub render_queue_delay: Option<f64>,

    /// Render background new file.
    /// Key: `renderbsnew`
    pub render_background_new: Option<u32>,

    /// Render tail length in ms.
    /// Key: `rendertail`
    pub render_tail: Option<u32>,

    /// Render tail length in ms (alternate).
    /// Key: `rendertaillen`
    pub render_tail_length: Option<u32>,

    /// Render tails mode.
    /// Key: `rendertails`
    pub render_tails: Option<u32>,

    /// Close render window when done.
    /// Key: `renderclosewhendone`
    pub render_close_when_done: Option<u32>,

    /// Render master track sub bitfield.
    /// Key: `rendermastertracksub`
    pub render_master_track_sub: Option<u32>,

    /// Render preview volume in dB.
    /// Key: `renderprvwvol`
    pub render_preview_volume: Option<f64>,

    // ── ReaScript ──
    /// ReaScript enable toggle.
    /// Key: `reascript`
    pub reascript: Option<u32>,

    /// ReaScript execution timeout in ms.
    /// Key: `reascripttimeout`
    pub reascript_timeout: Option<u32>,

    /// ReaScript watch update in ms.
    /// Key: `reascriptwatchms`
    pub reascript_watch_ms: Option<u32>,

    /// ReaScript IDE editor flags bitfield.
    /// Key: `edit_flags`
    pub edit_flags: Option<u32>,

    /// ReaScript editor suggestions.
    /// Key: `edit_sug`
    pub edit_suggestions: Option<u32>,

    /// ReaScript IDE font size.
    /// Key: `edit_fontsize`
    pub edit_font_size: Option<u32>,

    /// IDE font typeface.
    /// Key: `ide_font_face`
    pub ide_font_face: Option<String>,

    /// IDE color theme settings.
    /// Key: `ide_colors`
    pub ide_colors: Option<u32>,

    // ── ReaMote ──
    /// Enable ReaMote distributed processing.
    /// Key: `use_reamote`
    pub use_reamote: Option<bool>,

    /// ReaMote maximum block size.
    /// Key: `reamote_maxblock`
    pub reamote_max_block: Option<u32>,

    /// ReaMote max render latency in ms.
    /// Key: `reamote_maxlat_render`
    pub reamote_max_latency_render: Option<u32>,

    /// ReaMote maximum packet size.
    /// Key: `reamote_maxpkt`
    pub reamote_max_packet: Option<u32>,

    /// ReaMote sample format.
    /// Key: `reamote_smplfmt`
    pub reamote_sample_format: Option<u32>,
}

impl Rendering {
    pub(crate) fn from_ini(ini: &IniFile) -> Self {
        Self {
            render_config: get_u32(ini, S, "rendercfg"),
            bounce_config: get_u32(ini, S, "bouncecfg"),
            render_metadata_flags: get_u32(ini, S, "rendermetadataflags"),
            render_peaks: get_u32(ini, S, "renderpeaks"),
            render_queue_delay: get_f64(ini, S, "renderqdelay"),
            render_background_new: get_u32(ini, S, "renderbsnew"),
            render_tail: get_u32(ini, S, "rendertail"),
            render_tail_length: get_u32(ini, S, "rendertaillen"),
            render_tails: get_u32(ini, S, "rendertails"),
            render_close_when_done: get_u32(ini, S, "renderclosewhendone"),
            render_master_track_sub: get_u32(ini, S, "rendermastertracksub"),
            render_preview_volume: get_f64(ini, S, "renderprvwvol"),
            reascript: get_u32(ini, S, "reascript"),
            reascript_timeout: get_u32(ini, S, "reascripttimeout"),
            reascript_watch_ms: get_u32(ini, S, "reascriptwatchms"),
            edit_flags: get_u32(ini, S, "edit_flags"),
            edit_suggestions: get_u32(ini, S, "edit_sug"),
            edit_font_size: get_u32(ini, S, "edit_fontsize"),
            ide_font_face: get_string(ini, S, "ide_font_face"),
            ide_colors: get_u32(ini, S, "ide_colors"),
            use_reamote: get_bool(ini, S, "use_reamote"),
            reamote_max_block: get_u32(ini, S, "reamote_maxblock"),
            reamote_max_latency_render: get_u32(ini, S, "reamote_maxlat_render"),
            reamote_max_packet: get_u32(ini, S, "reamote_maxpkt"),
            reamote_sample_format: get_u32(ini, S, "reamote_smplfmt"),
        }
    }

    /// Returns a new [`Rendering`] with REAPER factory default values.
    ///
    /// Fields whose factory value is an empty string or floating-point value
    /// (when typed as `Option<u32>`) are left as `None`.
    pub fn factory_defaults() -> Self {
        Self {
            render_config: None,                            // rendercfg= (empty)
            bounce_config: None,                            // bouncecfg= (empty)
            render_metadata_flags: Some(0),                 // rendermetadataflags=0
            render_peaks: None,                             // renderpeaks not in factory defaults
            render_queue_delay: None,                       // renderqdelay not in factory defaults
            render_background_new: Some(0),                 // renderbsnew=0
            render_tail: Some(1000),                        // rendertail=1000
            render_tail_length: Some(1000),                 // rendertaillen=1000
            render_tails: Some(0),                          // rendertails=0
            render_close_when_done: None, // renderclosewhendone not in factory defaults
            render_master_track_sub: None, // rendermastertracksub not in factory defaults
            render_preview_volume: None,  // renderprvwvol not in factory defaults
            reascript: Some(0),           // reascript=0
            reascript_timeout: None, // reascripttimeout=0.00000000000000 (float, unparseable as u32)
            reascript_watch_ms: Some(500), // reascriptwatchms=500
            edit_flags: Some(0),     // edit_flags=0
            edit_suggestions: Some(50), // edit_sug=50
            edit_font_size: Some(14), // edit_fontsize=14
            ide_font_face: Some("Courier New".to_string()), // ide_font_face=Courier New
            ide_colors: None,        // ide_colors not in factory defaults
            use_reamote: None,       // use_reamote= (empty)
            reamote_max_block: None, // reamote_maxblock= (empty)
            reamote_max_latency_render: None, // reamote_maxlat_render= (empty)
            reamote_max_packet: None, // reamote_maxpkt= (empty)
            reamote_sample_format: None, // reamote_smplfmt= (empty)
        }
    }

    pub(crate) fn write_to(&self, ini: &mut IniFile) {
        set_opt_u32(ini, S, "rendercfg", self.render_config);
        set_opt_u32(ini, S, "bouncecfg", self.bounce_config);
        set_opt_u32(ini, S, "rendermetadataflags", self.render_metadata_flags);
        set_opt_u32(ini, S, "renderpeaks", self.render_peaks);
        set_opt_f64(ini, S, "renderqdelay", self.render_queue_delay);
        set_opt_u32(ini, S, "renderbsnew", self.render_background_new);
        set_opt_u32(ini, S, "rendertail", self.render_tail);
        set_opt_u32(ini, S, "rendertaillen", self.render_tail_length);
        set_opt_u32(ini, S, "rendertails", self.render_tails);
        set_opt_u32(ini, S, "renderclosewhendone", self.render_close_when_done);
        set_opt_u32(ini, S, "rendermastertracksub", self.render_master_track_sub);
        set_opt_f64(ini, S, "renderprvwvol", self.render_preview_volume);
        set_opt_u32(ini, S, "reascript", self.reascript);
        set_opt_u32(ini, S, "reascripttimeout", self.reascript_timeout);
        set_opt_u32(ini, S, "reascriptwatchms", self.reascript_watch_ms);
        set_opt_u32(ini, S, "edit_flags", self.edit_flags);
        set_opt_u32(ini, S, "edit_sug", self.edit_suggestions);
        set_opt_u32(ini, S, "edit_fontsize", self.edit_font_size);
        set_opt_string(ini, S, "ide_font_face", &self.ide_font_face);
        set_opt_u32(ini, S, "ide_colors", self.ide_colors);
        set_opt_bool(ini, S, "use_reamote", self.use_reamote);
        set_opt_u32(ini, S, "reamote_maxblock", self.reamote_max_block);
        set_opt_u32(
            ini,
            S,
            "reamote_maxlat_render",
            self.reamote_max_latency_render,
        );
        set_opt_u32(ini, S, "reamote_maxpkt", self.reamote_max_packet);
        set_opt_u32(ini, S, "reamote_smplfmt", self.reamote_sample_format);
    }
}
