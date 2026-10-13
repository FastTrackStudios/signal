//! Type definitions for REAPER render preset entries.

use serde::{Deserialize, Serialize};

/// A complete `reaper-render.ini` file, containing all render preset entries.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RenderPresetFile {
    /// The ordered list of top-level entries in the file.
    pub entries: Vec<RenderPresetEntry>,
}

/// A single top-level entry in `reaper-render.ini`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderPresetEntry {
    /// A `<RENDERPRESET ...>` block (primary format/settings).
    Preset(RenderPreset),
    /// A `<RENDERPRESET2 ...>` block (secondary output format).
    Preset2(RenderPreset2),
    /// A `RENDERPRESET_OUTPUT ...` line (output path and bounds).
    Output(RenderPresetOutput),
    /// A `RENDERPRESET_EXT ...` line (normalization and silence trimming).
    Ext(RenderPresetExt),
}

/// A `<RENDERPRESET>` block.
///
/// Stores the primary render format and quality settings for a named preset.
///
/// ```text
/// <RENDERPRESET presetname SampleRate channels offline_mode useprojectsamplerate resamplemode various_checkboxes various_checkboxes2
///   rendercfg_base64
/// >
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderPreset {
    /// Preset name (unquoted; spaces cause REAPER to quote it on write).
    pub name: String,
    /// Sample rate in Hz (e.g. 44100, 48000).
    pub sample_rate: u32,
    /// Number of output channels.
    pub channels: u32,
    /// Render mode / offline setting.
    ///
    /// - 0: Full-speed Offline
    /// - 1: 1x Offline
    /// - 2: Online Render
    /// - 3: Online Render (Idle)
    /// - 4: Offline Render (Idle)
    pub offline_mode: u32,
    /// Whether "Use project sample rate" checkbox is checked (0 = off, 1 = on).
    pub use_project_sample_rate: u32,
    /// Resample mode dropdown index.
    ///
    /// - 0: Medium (64pt Sinc)
    /// - 1: Low (Linear Interpolation)
    /// - 2: Lowest (Point Sampling)
    /// - 3: Good (192pt Sinc)
    /// - 4: Better (384pt Sinc)
    /// - 5: Fast (IIR + Linear Interpolation)
    /// - 6: Fast (IIR2 + Linear Interpolation)
    /// - 7: Fast (16pt Sinc)
    /// - 8: HQ (512pt Sinc)
    /// - 9: Extreme HQ (768pt Sinc)
    pub resample_mode: u32,
    /// Various checkboxes bitmask.
    ///
    /// - &1:  Dither Master
    /// - &2:  Noise shape master
    /// - &4:  Dither Stems
    /// - &8:  Noise shape stems
    pub various_checkboxes: u32,
    /// Various checkboxes 2 bitmask (added in Reaper 6).
    ///
    /// - &4:      Multichannel tracks to multichannel files
    /// - &16:     Tracks with only mono media to mono files
    /// - &256:    Embed stretch markers/transient guides
    /// - &1024:   Embed take markers
    /// - &2048:   2nd pass render
    /// - &8192:   Render stems pre-fader
    /// - &16384:  Only render channels that are sent to parent
    /// - &32768:  Preserve Metadata
    /// - &65536:  Preserve Start offset (selected media items)
    /// - &524288: Parallel render
    pub various_checkboxes2: u32,
    /// Render format settings encoded as a Base64 string.
    pub render_cfg: String,
}

/// A `<RENDERPRESET2>` block.
///
/// Stores a secondary output format for a named preset.
///
/// ```text
/// <RENDERPRESET2 presetname
///   rendercfg2_base64
/// >
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderPreset2 {
    /// Preset name (same as the corresponding `RenderPreset`).
    pub name: String,
    /// Secondary render format settings encoded as a Base64 string.
    pub render_cfg: String,
}

/// A `RENDERPRESET_OUTPUT` line.
///
/// Stores the output path, bounds, and tail settings for a named preset.
///
/// ```text
/// RENDERPRESET_OUTPUT presetname bounds start end source unknown filename tail dir tail_ms
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderPresetOutput {
    /// Preset name.
    pub name: String,
    /// Bounds dropdown selection.
    ///
    /// - 0: Custom time range
    /// - 1: Entire project
    /// - 2: Time selection
    /// - 3: Project regions
    /// - 5: Selected regions
    pub bounds: u32,
    /// Start position in seconds.
    pub start: f64,
    /// End position in seconds.
    pub end: f64,
    /// Source dropdown and checkboxes bitmask.
    ///
    /// - 0:    Master mix
    /// - 1:    Master mix + stems
    /// - 3:    Stems (selected tracks)
    /// - 8:    Region render matrix
    /// - 32:   Selected media items
    /// - 64:   Selected media items via master
    /// - 128:  Selected tracks via master
    /// - 4096: Razor Edit Areas
    pub source: u32,
    /// Unknown / legacy field (usually 0).
    pub unknown: u32,
    /// Output filename / render pattern string.
    pub filename: String,
    /// Whether the tail checkbox is checked for the current bounds (0 = off, 1 = on).
    pub tail: u32,
    /// Output directory (empty string when not set).
    pub directory: String,
    /// Tail length in milliseconds.
    pub tail_ms: u32,
}

/// A `RENDERPRESET_EXT` line.
///
/// Stores normalization, fade, and silence-trimming settings for a named preset.
///
/// ```text
/// RENDERPRESET_EXT presetname normalize_mode normalize_target brickwall_target
///     fadein_length fadeout_length fadein_shape fadeout_shape
///     trim_lead trim_trail pad_start pad_end
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderPresetExt {
    /// Preset name.
    pub name: String,
    /// Normalization mode bitmask.
    ///
    /// - &1:      Enable normalizing
    /// - &6:      Normalize algorithm (0=LUFS-I, 2=RMS-I, 4=Peak, 6=True Peak, 8=LUFS-M max, 10=LUFS-S max)
    /// - &32:     Normalize stems to master target
    /// - &64:     Brickwall limiting
    /// - &128:    Brickwall Limiting mode (0=Peak, 1=True Peak)
    /// - &256:    Only normalize files that are too loud
    /// - &512:    Fade-in enabled
    /// - &1024:   Fade-out enabled
    /// - &16384:  Trim leading silence (0=enabled, 1=disabled)
    /// - &32768:  Trim trailing silence (0=enabled, 1=disabled)
    /// - &65536:  Pad start with silence (0=enabled, 1=disabled)
    /// - &131072: Pad end with silence (0=enabled, 1=disabled)
    pub normalize_mode: u32,
    /// Normalize target as MKVOL value.
    pub normalize_target: f64,
    /// Brickwall target as MKVOL value.
    pub brickwall_target: f64,
    /// Fade-in length in seconds.
    pub fadein_length: f64,
    /// Fade-out length in seconds.
    pub fadeout_length: f64,
    /// Fade-in curve shape (0=linear … 6=Quartic S-curve).
    pub fadein_shape: u32,
    /// Fade-out curve shape (0=linear … 6=Quartic S-curve).
    pub fadeout_shape: u32,
    /// Trim leading silence threshold (MKVOL value).
    pub trim_lead: f64,
    /// Trim trailing silence threshold (MKVOL value).
    pub trim_trail: f64,
    /// Pad start with silence duration in seconds (dialog shows milliseconds).
    pub pad_start: f64,
    /// Pad end with silence duration in seconds (dialog shows milliseconds).
    pub pad_end: f64,
}
