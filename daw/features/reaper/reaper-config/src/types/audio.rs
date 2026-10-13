//! Audio device and buffering preferences (Preferences > Audio).

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::ini::IniFile;
use crate::parse::*;
use crate::types::common::{AudioThreadPriority, DiskReadMode, DiskWriteMode};

const S: &str = "REAPER";

/// Tiny hardware fade-in/fade-out on playback start/stop.
///
/// Corresponds to REAPER's `hwfadex` INI key. Both are on by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Facet)]
pub struct HardwareFadeFlags {
    /// Apply a tiny fade-out when playback stops. Default: `true`.
    pub fade_out_on_stop: bool,
    /// Apply a tiny fade-in when playback starts. Default: `true`.
    pub fade_in_on_start: bool,
}

impl Default for HardwareFadeFlags {
    fn default() -> Self {
        Self {
            fade_out_on_stop: true,
            fade_in_on_start: true,
        }
    }
}

impl HardwareFadeFlags {
    /// Encode as the `hwfadex` INI integer.
    pub fn to_u32(self) -> u32 {
        let mut v = 0u32;
        if self.fade_out_on_stop {
            v |= 1;
        }
        if self.fade_in_on_start {
            v |= 2;
        }
        v
    }

    /// Decode from the `hwfadex` INI integer.
    pub fn from_u32(v: u32) -> Self {
        Self {
            fade_out_on_stop: (v & 1) != 0,
            fade_in_on_start: (v & 2) != 0,
        }
    }
}

/// When to automatically close the audio device.
///
/// Corresponds to REAPER's `audiocloseinactive` INI key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Facet, Default)]
pub struct AudioCloseInactiveFlags {
    /// Close audio device when stopped and application is inactive. Default: `false`.
    pub close_when_inactive: bool,
    /// Close audio device when stopped and application is minimized. Default: `false`.
    pub close_when_minimized: bool,
}

impl AudioCloseInactiveFlags {
    /// Encode as the `audiocloseinactive` INI integer.
    pub fn to_u32(self) -> u32 {
        let mut v = 0u32;
        if self.close_when_inactive {
            v |= 1;
        }
        if self.close_when_minimized {
            v |= 2;
        }
        v
    }

    /// Decode from the `audiocloseinactive` INI integer.
    pub fn from_u32(v: u32) -> Self {
        Self {
            close_when_inactive: (v & 1) != 0,
            close_when_minimized: (v & 2) != 0,
        }
    }
}

/// Audio device and buffering preferences.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Facet)]
pub struct Audio {
    /// Audio synchronization settings bitfield.
    /// Key: `audioasync`
    pub audio_async: Option<u32>,

    /// Audio thread priority level.
    /// Key: `audiothreadpr`
    #[facet(opaque)]
    pub audio_thread_priority: Option<AudioThreadPriority>,

    /// When to automatically close the audio device.
    /// Key: `audiocloseinactive`
    pub audio_close_inactive: Option<AudioCloseInactiveFlags>,

    /// Close audio device when stopped.
    /// Key: `audioclosestop`
    pub audio_close_stop: Option<bool>,

    /// Show non-standard stereo pairs.
    /// Key: `allstereopairs`
    pub all_stereo_pairs: Option<bool>,

    /// Input channel aliasing options bitfield.
    /// Key: `useinnc`
    pub use_input_channel_aliasing: Option<u32>,

    /// Tiny fade in/out on playback start/stop.
    /// Key: `hwfadex`
    pub hardware_fade: Option<HardwareFadeFlags>,

    /// Default metronome output channel bitfield.
    /// Key: `metronome_defout`
    pub metronome_default_output: Option<u32>,

    /// Metronome monitor FX settings.
    /// Key: `metronome_flags`
    pub metronome_flags: Option<u32>,

    /// Silent track CPU optimization bitfield.
    /// Key: `optimizesilence`
    pub optimize_silence: Option<u32>,

    /// PDC auto-bypass threshold in ms.
    /// Key: `pdcautobypassms`
    pub pdc_auto_bypass_ms: Option<f64>,

    /// Control surface update frequency in Hz.
    /// Key: `csurfrate`
    pub control_surface_rate: Option<u32>,

    // ── Disk I/O ──
    /// Memory map peak files toggle.
    /// Key: `disk_peakmmap2`
    pub disk_peak_mmap: Option<bool>,

    /// Peak file memory map buffer in KB.
    /// Key: `disk_peaks_mmapkb`
    pub disk_peaks_mmap_kb: Option<u32>,

    /// Peak file RAM buffer size in KB.
    /// Key: `disk_peaks_ramkb`
    pub disk_peaks_ram_kb: Option<u32>,

    /// Disk read buffer count bitfield.
    /// Key: `disk_rdblksex`
    pub disk_read_blocks: Option<u32>,

    /// Preferred disk read mode.
    /// Key: `disk_rdmodeex`
    #[facet(opaque)]
    pub disk_read_mode: Option<DiskReadMode>,

    /// Preferred disk read mode (macOS-specific override).
    /// Key: `disk_rdmodeexmac`
    #[facet(opaque)]
    pub disk_read_mode_mac: Option<DiskReadMode>,

    /// Disk read buffer size in bytes.
    /// Key: `disk_rdsizeex`
    pub disk_read_size: Option<u32>,

    /// Write buffer count (default).
    /// Key: `disk_wrblks`
    pub disk_write_blocks: Option<u32>,

    /// Write buffer count (maximum).
    /// Key: `disk_wrblks2`
    pub disk_write_blocks_max: Option<u32>,

    /// Preferred disk write mode.
    /// Key: `disk_wrmode`
    #[facet(opaque)]
    pub disk_write_mode: Option<DiskWriteMode>,

    /// Disk write buffer size in bytes.
    /// Key: `disk_wrsize`
    pub disk_write_size: Option<u32>,
}

impl Audio {
    pub(crate) fn from_ini(ini: &IniFile) -> Self {
        Self {
            audio_async: get_u32(ini, S, "audioasync"),
            audio_thread_priority: get_enum_i32(ini, S, "audiothreadpr"),
            audio_close_inactive: get_u32(ini, S, "audiocloseinactive")
                .map(AudioCloseInactiveFlags::from_u32),
            audio_close_stop: get_bool(ini, S, "audioclosestop"),
            all_stereo_pairs: get_bool(ini, S, "allstereopairs"),
            use_input_channel_aliasing: get_u32(ini, S, "useinnc"),
            hardware_fade: get_u32(ini, S, "hwfadex").map(HardwareFadeFlags::from_u32),
            metronome_default_output: get_u32(ini, S, "metronome_defout"),
            metronome_flags: get_u32(ini, S, "metronome_flags"),
            optimize_silence: get_u32(ini, S, "optimizesilence"),
            pdc_auto_bypass_ms: get_f64(ini, S, "pdcautobypassms"),
            control_surface_rate: get_u32(ini, S, "csurfrate"),
            disk_peak_mmap: get_bool(ini, S, "disk_peakmmap2"),
            disk_peaks_mmap_kb: get_u32(ini, S, "disk_peaks_mmapkb"),
            disk_peaks_ram_kb: get_u32(ini, S, "disk_peaks_ramkb"),
            disk_read_blocks: get_u32(ini, S, "disk_rdblksex"),
            disk_read_mode: get_enum_u32(ini, S, "disk_rdmodeex"),
            disk_read_mode_mac: get_enum_u32(ini, S, "disk_rdmodeexmac"),
            disk_read_size: get_u32(ini, S, "disk_rdsizeex"),
            disk_write_blocks: get_u32(ini, S, "disk_wrblks"),
            disk_write_blocks_max: get_u32(ini, S, "disk_wrblks2"),
            disk_write_mode: get_enum_u32(ini, S, "disk_wrmode"),
            disk_write_size: get_u32(ini, S, "disk_wrsize"),
        }
    }

    /// Returns a new [`Audio`] with REAPER factory default values.
    ///
    /// Fields whose factory value is an empty string are left as `None`.
    /// Fields with negative integer defaults that are typed as `Option<u32>`
    /// are also `None` because the INI parser cannot represent them as `u32`.
    pub fn factory_defaults() -> Self {
        Self {
            audio_async: None, // audioasync= (empty)
            audio_thread_priority: Some(AudioThreadPriority::AsioDefault), // audiothreadpr=-1
            audio_close_inactive: None, // audiocloseinactive= (empty — both flags off)
            audio_close_stop: Some(false), // audioclosestop=0
            all_stereo_pairs: Some(true), // allstereopairs=1
            use_input_channel_aliasing: Some(0), // useinnc=0
            hardware_fade: Some(HardwareFadeFlags::default()), // hwfadex=3 (both on)
            metronome_default_output: None, // metronome_defout=-1 (negative, unparseable as u32)
            metronome_flags: Some(0), // metronome_flags=0
            optimize_silence: Some(0), // optimizesilence=0
            pdc_auto_bypass_ms: Some(0.0), // pdcautobypassms=0.00000000000000
            control_surface_rate: Some(15), // csurfrate=15
            disk_peak_mmap: None, // disk_peakmmap2= (empty)
            disk_peaks_mmap_kb: Some(0), // disk_peaks_mmapkb=0
            disk_peaks_ram_kb: Some(16), // disk_peaks_ramkb=16
            disk_read_blocks: Some(3), // disk_rdblksex=3
            disk_read_mode: None, // disk_rdmodeex= (empty — defaults to AsyncBuffered)
            disk_read_mode_mac: None, // disk_rdmodeexmac not in factory defaults
            disk_read_size: None, // disk_rdsizeex not in factory defaults
            disk_write_blocks: Some(8), // disk_wrblks=8
            disk_write_blocks_max: Some(128), // disk_wrblks2=128
            disk_write_mode: Some(DiskWriteMode::AsyncWriteThrough), // disk_wrmode=2
            disk_write_size: Some(65536), // disk_wrsize=65536
        }
    }

    pub(crate) fn write_to(&self, ini: &mut IniFile) {
        set_opt_u32(ini, S, "audioasync", self.audio_async);
        set_opt_enum_i32(ini, S, "audiothreadpr", self.audio_thread_priority);
        if let Some(f) = self.audio_close_inactive {
            ini.set(S, "audiocloseinactive", &f.to_u32().to_string());
        }
        set_opt_bool(ini, S, "audioclosestop", self.audio_close_stop);
        set_opt_bool(ini, S, "allstereopairs", self.all_stereo_pairs);
        set_opt_u32(ini, S, "useinnc", self.use_input_channel_aliasing);
        if let Some(f) = self.hardware_fade {
            ini.set(S, "hwfadex", &f.to_u32().to_string());
        }
        set_opt_u32(ini, S, "metronome_defout", self.metronome_default_output);
        set_opt_u32(ini, S, "metronome_flags", self.metronome_flags);
        set_opt_u32(ini, S, "optimizesilence", self.optimize_silence);
        set_opt_f64(ini, S, "pdcautobypassms", self.pdc_auto_bypass_ms);
        set_opt_u32(ini, S, "csurfrate", self.control_surface_rate);
        set_opt_bool(ini, S, "disk_peakmmap2", self.disk_peak_mmap);
        set_opt_u32(ini, S, "disk_peaks_mmapkb", self.disk_peaks_mmap_kb);
        set_opt_u32(ini, S, "disk_peaks_ramkb", self.disk_peaks_ram_kb);
        set_opt_u32(ini, S, "disk_rdblksex", self.disk_read_blocks);
        set_opt_enum_u32(ini, S, "disk_rdmodeex", self.disk_read_mode);
        set_opt_enum_u32(ini, S, "disk_rdmodeexmac", self.disk_read_mode_mac);
        set_opt_u32(ini, S, "disk_rdsizeex", self.disk_read_size);
        set_opt_u32(ini, S, "disk_wrblks", self.disk_write_blocks);
        set_opt_u32(ini, S, "disk_wrblks2", self.disk_write_blocks_max);
        set_opt_enum_u32(ini, S, "disk_wrmode", self.disk_write_mode);
        set_opt_u32(ini, S, "disk_wrsize", self.disk_write_size);
    }
}
