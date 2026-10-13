//! Shared enums used across multiple REAPER configuration modules.
//!
//! These enums replace raw integer fields with type-safe alternatives.
//! Each enum implements `From<i32>`/`Into<i32>` (or `u32` where the
//! underlying INI type is unsigned) to support transparent parsing and
//! serialization to/from the INI file.

use serde::{Deserialize, Serialize};

// ────────────────────────────────────────────────────────────────────────────
// TimeDisplayFormat
// ────────────────────────────────────────────────────────────────────────────

/// Ruler / transport time-display format.
///
/// Maps to REAPER's `projtimemode` and `projtimemode2` INI keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeDisplayFormat {
    /// Displayed as `mm:ss.ms` (minutes, seconds, milliseconds).
    MinsSecs,
    /// Displayed as measures and beats.
    MeasuresBeats,
    /// Displayed as elapsed seconds.
    Seconds,
    /// Displayed as sample count.
    Samples,
    /// Displayed as `HH:MM:SS:FF` (hours, minutes, seconds, frames).
    HmsFrames,
    /// Displayed as an absolute frame number.
    AbsoluteFrames,
    /// A value not recognized by this crate.
    Unknown(i32),
}

impl From<i32> for TimeDisplayFormat {
    fn from(v: i32) -> Self {
        match v {
            0 => Self::MinsSecs,
            1 => Self::MeasuresBeats,
            2 => Self::Seconds,
            3 => Self::Samples,
            4 => Self::HmsFrames,
            5 => Self::AbsoluteFrames,
            _ => Self::Unknown(v),
        }
    }
}

impl From<TimeDisplayFormat> for i32 {
    fn from(v: TimeDisplayFormat) -> Self {
        match v {
            TimeDisplayFormat::MinsSecs => 0,
            TimeDisplayFormat::MeasuresBeats => 1,
            TimeDisplayFormat::Seconds => 2,
            TimeDisplayFormat::Samples => 3,
            TimeDisplayFormat::HmsFrames => 4,
            TimeDisplayFormat::AbsoluteFrames => 5,
            TimeDisplayFormat::Unknown(n) => n,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// RippleMode
// ────────────────────────────────────────────────────────────────────────────

/// Ripple-edit mode.
///
/// Maps to REAPER's `projripedit` INI key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RippleMode {
    /// Ripple editing disabled.
    Off,
    /// Ripple editing applies only to the selected track.
    PerTrack,
    /// Ripple editing applies to all tracks simultaneously.
    AllTracks,
    /// A value not recognized by this crate.
    Unknown(i32),
}

impl From<i32> for RippleMode {
    fn from(v: i32) -> Self {
        match v {
            0 => Self::Off,
            1 => Self::PerTrack,
            2 => Self::AllTracks,
            _ => Self::Unknown(v),
        }
    }
}

impl From<RippleMode> for i32 {
    fn from(v: RippleMode) -> Self {
        match v {
            RippleMode::Off => 0,
            RippleMode::PerTrack => 1,
            RippleMode::AllTracks => 2,
            RippleMode::Unknown(n) => n,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// PanMode
// ────────────────────────────────────────────────────────────────────────────

/// Stereo pan algorithm used for a track or the master.
///
/// Maps to REAPER's `panmode` and `projmasterpanmode` INI keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PanMode {
    /// Classic REAPER balance/pan (linear law, simple left-right balance).
    Classic,
    /// Balanced pan (equal-power with balance control).
    Balanced,
    /// Stereo pan (independent control of each channel level).
    StereoPan,
    /// Dual-pan (separate left and right pan knobs).
    DualPan,
    /// MIDI pan (follows MIDI CC10 pan specification).
    MidiPan,
    /// A value not recognized by this crate.
    Unknown(i32),
}

impl From<i32> for PanMode {
    fn from(v: i32) -> Self {
        match v {
            0 => Self::Classic,
            1 => Self::Balanced,
            3 => Self::StereoPan,
            5 => Self::DualPan,
            6 => Self::MidiPan,
            _ => Self::Unknown(v),
        }
    }
}

impl From<PanMode> for i32 {
    fn from(v: PanMode) -> Self {
        match v {
            PanMode::Classic => 0,
            PanMode::Balanced => 1,
            PanMode::StereoPan => 3,
            PanMode::DualPan => 5,
            PanMode::MidiPan => 6,
            PanMode::Unknown(n) => n,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// AutomationMode
// ────────────────────────────────────────────────────────────────────────────

/// Track automation mode.
///
/// Maps to REAPER's `defautomode` and `envwritepasschg` INI keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutomationMode {
    /// Trim/read: applies a static offset on top of existing automation.
    TrimRead,
    /// Read: plays back existing automation envelopes.
    Read,
    /// Touch: overwrites automation only while a parameter is being moved.
    Touch,
    /// Write: continuously overwrites automation during playback.
    Write,
    /// Latch: overwrites automation from the first parameter touch onward.
    Latch,
    /// Latch preview: like Latch, but previews changes before committing.
    LatchPreview,
    /// A value not recognized by this crate.
    Unknown(i32),
}

impl From<i32> for AutomationMode {
    fn from(v: i32) -> Self {
        match v {
            0 => Self::TrimRead,
            1 => Self::Read,
            2 => Self::Touch,
            3 => Self::Write,
            4 => Self::Latch,
            5 => Self::LatchPreview,
            _ => Self::Unknown(v),
        }
    }
}

impl From<AutomationMode> for i32 {
    fn from(v: AutomationMode) -> Self {
        match v {
            AutomationMode::TrimRead => 0,
            AutomationMode::Read => 1,
            AutomationMode::Touch => 2,
            AutomationMode::Write => 3,
            AutomationMode::Latch => 4,
            AutomationMode::LatchPreview => 5,
            AutomationMode::Unknown(n) => n,
        }
    }
}

impl From<u32> for AutomationMode {
    fn from(v: u32) -> Self {
        Self::from(v as i32)
    }
}

impl From<AutomationMode> for u32 {
    fn from(v: AutomationMode) -> Self {
        i32::from(v) as u32
    }
}

// ────────────────────────────────────────────────────────────────────────────
// ResampleQuality
// ────────────────────────────────────────────────────────────────────────────

/// Playback resampling quality.
///
/// Maps to REAPER's `playresamplemode` INI key.
/// Higher values give better audio quality at the cost of CPU usage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResampleQuality {
    Lowest,
    Low,
    Medium,
    MediumHigh,
    High,
    VeryHigh,
    Highest,
    Best,
    /// A value not recognized by this crate.
    Unknown(i32),
}

impl From<i32> for ResampleQuality {
    fn from(v: i32) -> Self {
        match v {
            0 => Self::Lowest,
            1 => Self::Low,
            2 => Self::Medium,
            3 => Self::MediumHigh,
            4 => Self::High,
            5 => Self::VeryHigh,
            6 => Self::Highest,
            7 => Self::Best,
            _ => Self::Unknown(v),
        }
    }
}

impl From<ResampleQuality> for i32 {
    fn from(v: ResampleQuality) -> Self {
        match v {
            ResampleQuality::Lowest => 0,
            ResampleQuality::Low => 1,
            ResampleQuality::Medium => 2,
            ResampleQuality::MediumHigh => 3,
            ResampleQuality::High => 4,
            ResampleQuality::VeryHigh => 5,
            ResampleQuality::Highest => 6,
            ResampleQuality::Best => 7,
            ResampleQuality::Unknown(n) => n,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// FadeCurveType
// ────────────────────────────────────────────────────────────────────────────

/// Fade / crossfade curve shape.
///
/// Maps to REAPER's `deffadeshape` and `defxfadeshape` INI keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FadeCurveType {
    /// Linear fade.
    Linear,
    /// Square (power) fade.
    Square,
    /// Slow start and slow end (S-curve).
    SlowStartEnd,
    /// Fast start (logarithmic).
    FastStart,
    /// Fast end (exponential).
    FastEnd,
    /// Bézier curve.
    Bezier,
    /// A value not recognized by this crate.
    Unknown(u32),
}

impl From<u32> for FadeCurveType {
    fn from(v: u32) -> Self {
        match v {
            0 => Self::Linear,
            1 => Self::Square,
            2 => Self::SlowStartEnd,
            3 => Self::FastStart,
            4 => Self::FastEnd,
            5 => Self::Bezier,
            _ => Self::Unknown(v),
        }
    }
}

impl From<FadeCurveType> for u32 {
    fn from(v: FadeCurveType) -> Self {
        match v {
            FadeCurveType::Linear => 0,
            FadeCurveType::Square => 1,
            FadeCurveType::SlowStartEnd => 2,
            FadeCurveType::FastStart => 3,
            FadeCurveType::FastEnd => 4,
            FadeCurveType::Bezier => 5,
            FadeCurveType::Unknown(n) => n,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// DiskReadMode
// ────────────────────────────────────────────────────────────────────────────

/// Preferred disk read mode for audio playback.
///
/// Maps to REAPER's `disk_rdmodeex` INI key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiskReadMode {
    /// Asynchronous, unbuffered I/O.
    AsyncUnbuffered,
    /// Asynchronous, buffered I/O (default).
    AsyncBuffered,
    /// Synchronous I/O.
    Sync,
    /// A value not recognized by this crate.
    Unknown(u32),
}

impl From<u32> for DiskReadMode {
    fn from(v: u32) -> Self {
        match v {
            0 => Self::AsyncUnbuffered,
            1 => Self::AsyncBuffered,
            2 => Self::Sync,
            _ => Self::Unknown(v),
        }
    }
}

impl From<DiskReadMode> for u32 {
    fn from(v: DiskReadMode) -> Self {
        match v {
            DiskReadMode::AsyncUnbuffered => 0,
            DiskReadMode::AsyncBuffered => 1,
            DiskReadMode::Sync => 2,
            DiskReadMode::Unknown(n) => n,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// DiskWriteMode
// ────────────────────────────────────────────────────────────────────────────

/// Preferred disk write mode for audio recording.
///
/// Maps to REAPER's `disk_wrmode` INI key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiskWriteMode {
    /// Asynchronous (default).
    Async,
    /// Synchronous.
    Sync,
    /// Asynchronous with write-through cache.
    AsyncWriteThrough,
    /// A value not recognized by this crate.
    Unknown(u32),
}

impl From<u32> for DiskWriteMode {
    fn from(v: u32) -> Self {
        match v {
            0 => Self::Async,
            1 => Self::Sync,
            2 => Self::AsyncWriteThrough,
            _ => Self::Unknown(v),
        }
    }
}

impl From<DiskWriteMode> for u32 {
    fn from(v: DiskWriteMode) -> Self {
        match v {
            DiskWriteMode::Async => 0,
            DiskWriteMode::Sync => 1,
            DiskWriteMode::AsyncWriteThrough => 2,
            DiskWriteMode::Unknown(n) => n,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// AutoSaveMode
// ────────────────────────────────────────────────────────────────────────────

/// Auto-save trigger condition.
///
/// Maps to REAPER's `autosavemode` INI key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutoSaveMode {
    /// Auto-save only when not recording (default).
    WhenNotRecording,
    /// Auto-save only when transport is stopped.
    WhenStopped,
    /// Auto-save at any time, including during playback and recording.
    AnyTime,
    /// A value not recognized by this crate.
    Unknown(u32),
}

impl From<u32> for AutoSaveMode {
    fn from(v: u32) -> Self {
        match v {
            0 => Self::WhenNotRecording,
            1 => Self::WhenStopped,
            2 => Self::AnyTime,
            _ => Self::Unknown(v),
        }
    }
}

impl From<AutoSaveMode> for u32 {
    fn from(v: AutoSaveMode) -> Self {
        match v {
            AutoSaveMode::WhenNotRecording => 0,
            AutoSaveMode::WhenStopped => 1,
            AutoSaveMode::AnyTime => 2,
            AutoSaveMode::Unknown(n) => n,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// EnvelopeTrimMode
// ────────────────────────────────────────────────────────────────────────────

/// When adding a volume/pan envelope, how to handle applying trim and resetting it.
///
/// Maps to REAPER's `envtrimadjmode` INI key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnvelopeTrimMode {
    /// Always apply trim to envelope and reset trim (default).
    Always,
    /// Apply trim only when in read or write automation modes.
    InReadWrite,
    /// Never apply trim to the envelope.
    Never,
    /// A value not recognized by this crate.
    Unknown(u32),
}

impl From<u32> for EnvelopeTrimMode {
    fn from(v: u32) -> Self {
        match v {
            0 => Self::Always,
            1 => Self::InReadWrite,
            2 => Self::Never,
            _ => Self::Unknown(v),
        }
    }
}

impl From<EnvelopeTrimMode> for u32 {
    fn from(v: EnvelopeTrimMode) -> Self {
        match v {
            EnvelopeTrimMode::Always => 0,
            EnvelopeTrimMode::InReadWrite => 1,
            EnvelopeTrimMode::Never => 2,
            EnvelopeTrimMode::Unknown(n) => n,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// VstBridgeMode
// ────────────────────────────────────────────────────────────────────────────

/// VST plug-in bridging / firewall mode.
///
/// Maps to REAPER's `vstbr64` INI key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VstBridgeMode {
    /// Bridge automatically when required (default).
    Auto,
    /// Run all bridged plug-ins in a single shared separate process.
    SeparateProcess,
    /// Run each bridged plug-in in its own dedicated process.
    DedicatedProcess,
    /// Native only — disable bridging entirely.
    NativeOnly,
    /// A value not recognized by this crate.
    Unknown(u32),
}

impl From<u32> for VstBridgeMode {
    fn from(v: u32) -> Self {
        match v {
            0 => Self::Auto,
            1 => Self::SeparateProcess,
            2 => Self::DedicatedProcess,
            3 => Self::NativeOnly,
            _ => Self::Unknown(v),
        }
    }
}

impl From<VstBridgeMode> for u32 {
    fn from(v: VstBridgeMode) -> Self {
        match v {
            VstBridgeMode::Auto => 0,
            VstBridgeMode::SeparateProcess => 1,
            VstBridgeMode::DedicatedProcess => 2,
            VstBridgeMode::NativeOnly => 3,
            VstBridgeMode::Unknown(n) => n,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// AutoMuteMode
// ────────────────────────────────────────────────────────────────────────────

/// Automatic muting behavior when tracks fall below the threshold.
///
/// Maps to REAPER's `automute` INI key (stored in `[Mute]` section).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutoMuteMode {
    /// No automatic muting.
    Off,
    /// Automatically mute the master track only.
    MuteMaster,
    /// Automatically mute any track below the threshold.
    MuteAny,
    /// A value not recognized by this crate.
    Unknown(u32),
}

impl From<u32> for AutoMuteMode {
    fn from(v: u32) -> Self {
        match v {
            0 => Self::Off,
            1 => Self::MuteMaster,
            2 => Self::MuteAny,
            _ => Self::Unknown(v),
        }
    }
}

impl From<AutoMuteMode> for u32 {
    fn from(v: AutoMuteMode) -> Self {
        match v {
            AutoMuteMode::Off => 0,
            AutoMuteMode::MuteMaster => 1,
            AutoMuteMode::MuteAny => 2,
            AutoMuteMode::Unknown(n) => n,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// ZLayer
// ────────────────────────────────────────────────────────────────────────────

/// Z-order (depth) of grid lines and markers relative to media items.
///
/// Maps to REAPER's `gridinbg` and `gridinbg2` INI keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ZLayer {
    /// Draw over items (in front).
    OverItems,
    /// Draw through items (interleaved).
    ThroughItems,
    /// Draw under items (behind).
    UnderItems,
    /// A value not recognized by this crate.
    Unknown(u32),
}

impl From<u32> for ZLayer {
    fn from(v: u32) -> Self {
        match v {
            0 => Self::OverItems,
            1 => Self::ThroughItems,
            2 => Self::UnderItems,
            _ => Self::Unknown(v),
        }
    }
}

impl From<ZLayer> for u32 {
    fn from(v: ZLayer) -> Self {
        match v {
            ZLayer::OverItems => 0,
            ZLayer::ThroughItems => 1,
            ZLayer::UnderItems => 2,
            ZLayer::Unknown(n) => n,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// HelpDisplay
// ────────────────────────────────────────────────────────────────────────────

/// What information is displayed in the help area below the TCP.
///
/// Maps to REAPER's `help` INI key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HelpDisplay {
    /// No information displayed.
    None,
    /// Show REAPER tips.
    Tips,
    /// Show track and item counts.
    TrackItemCount,
    /// Show details of the selected track, item, or envelope.
    SelectedDetails,
    /// A value not recognized by this crate.
    Unknown(u32),
}

impl From<u32> for HelpDisplay {
    fn from(v: u32) -> Self {
        match v {
            0 => Self::None,
            1 => Self::Tips,
            2 => Self::TrackItemCount,
            3 => Self::SelectedDetails,
            _ => Self::Unknown(v),
        }
    }
}

impl From<HelpDisplay> for u32 {
    fn from(v: HelpDisplay) -> Self {
        match v {
            HelpDisplay::None => 0,
            HelpDisplay::Tips => 1,
            HelpDisplay::TrackItemCount => 2,
            HelpDisplay::SelectedDetails => 3,
            HelpDisplay::Unknown(n) => n,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// ModalWindowPosition
// ────────────────────────────────────────────────────────────────────────────

/// Where modal dialogs (preferences, etc.) are positioned on screen.
///
/// Maps to REAPER's `windowflags` INI key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModalWindowPosition {
    /// Re-open at the last saved position (default).
    LastPosition,
    /// Center on the current screen.
    CenterCurrentScreen,
    /// Center on the mouse cursor position.
    CenterMouseCursor,
    /// Let the OS decide the position.
    OsPositioning,
    /// A value not recognized by this crate.
    Unknown(u32),
}

impl From<u32> for ModalWindowPosition {
    fn from(v: u32) -> Self {
        match v {
            0 => Self::LastPosition,
            1 => Self::CenterCurrentScreen,
            2 => Self::CenterMouseCursor,
            3 => Self::OsPositioning,
            _ => Self::Unknown(v),
        }
    }
}

impl From<ModalWindowPosition> for u32 {
    fn from(v: ModalWindowPosition) -> Self {
        match v {
            ModalWindowPosition::LastPosition => 0,
            ModalWindowPosition::CenterCurrentScreen => 1,
            ModalWindowPosition::CenterMouseCursor => 2,
            ModalWindowPosition::OsPositioning => 3,
            ModalWindowPosition::Unknown(n) => n,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// ItemVolumeHandlePosition
// ────────────────────────────────────────────────────────────────────────────

/// Where the volume handle appears on a media item.
///
/// Maps to REAPER's `itemvolmode` INI key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemVolumeHandlePosition {
    /// Handle at the top of the item (+0 dB = top of item).
    AtTop,
    /// Handle at the center of the item (+0 dB = center).
    AtCenter,
    /// A value not recognized by this crate.
    Unknown(u32),
}

impl From<u32> for ItemVolumeHandlePosition {
    fn from(v: u32) -> Self {
        match v {
            0 => Self::AtTop,
            1 => Self::AtCenter,
            _ => Self::Unknown(v),
        }
    }
}

impl From<ItemVolumeHandlePosition> for u32 {
    fn from(v: ItemVolumeHandlePosition) -> Self {
        match v {
            ItemVolumeHandlePosition::AtTop => 0,
            ItemVolumeHandlePosition::AtCenter => 1,
            ItemVolumeHandlePosition::Unknown(n) => n,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// InsertMasterTrackMode
// ────────────────────────────────────────────────────────────────────────────

/// How newly imported media items are inserted relative to tracks.
///
/// Maps to REAPER's `insertmtrack` INI key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InsertMasterTrackMode {
    /// Insert all items in one track, advancing time (default).
    OneTrack,
    /// Insert items each on a separate track.
    AcrossTracks,
    /// Decide automatically based on media length.
    Auto,
    /// Prompt the user each time.
    Prompt,
    /// A value not recognized by this crate.
    Unknown(u32),
}

impl From<u32> for InsertMasterTrackMode {
    fn from(v: u32) -> Self {
        match v {
            0 => Self::OneTrack,
            1 => Self::AcrossTracks,
            2 => Self::Auto,
            3 => Self::Prompt,
            _ => Self::Unknown(v),
        }
    }
}

impl From<InsertMasterTrackMode> for u32 {
    fn from(v: InsertMasterTrackMode) -> Self {
        match v {
            InsertMasterTrackMode::OneTrack => 0,
            InsertMasterTrackMode::AcrossTracks => 1,
            InsertMasterTrackMode::Auto => 2,
            InsertMasterTrackMode::Prompt => 3,
            InsertMasterTrackMode::Unknown(n) => n,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// AcidImportMode
// ────────────────────────────────────────────────────────────────────────────

/// How to handle ACID-format media with embedded tempo during import.
///
/// Maps to REAPER's `acidimport` INI key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AcidImportMode {
    /// Adjust imported media to match the project tempo (default).
    AdjustToProject,
    /// Import media at its original source tempo.
    SourceTempo,
    /// Always prompt when importing media with embedded tempo.
    AlwaysPrompt,
    /// A value not recognized by this crate.
    Unknown(u32),
}

impl From<u32> for AcidImportMode {
    fn from(v: u32) -> Self {
        match v {
            0 => Self::AdjustToProject,
            1 => Self::SourceTempo,
            2 => Self::AlwaysPrompt,
            _ => Self::Unknown(v),
        }
    }
}

impl From<AcidImportMode> for u32 {
    fn from(v: AcidImportMode) -> Self {
        match v {
            AcidImportMode::AdjustToProject => 0,
            AcidImportMode::SourceTempo => 1,
            AcidImportMode::AlwaysPrompt => 2,
            AcidImportMode::Unknown(n) => n,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// AudioThreadPriority
// ────────────────────────────────────────────────────────────────────────────

/// Audio processing thread priority level.
///
/// Maps to REAPER's `audiothreadpr` INI key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioThreadPriority {
    /// ASIO default / MMCSS Pro Audio / Time Critical.
    AsioDefault,
    /// Normal OS thread priority.
    Normal,
    /// Above normal OS thread priority.
    AboveNormal,
    /// Highest OS thread priority.
    Highest,
    /// Time-critical OS thread priority.
    TimeCritical,
    /// MMCSS / Time Critical (Windows only).
    MmcssTimeCritical,
    /// A value not recognized by this crate.
    Unknown(i32),
}

impl From<i32> for AudioThreadPriority {
    fn from(v: i32) -> Self {
        match v {
            -1 => Self::AsioDefault,
            0 => Self::Normal,
            1 => Self::AboveNormal,
            2 => Self::Highest,
            3 => Self::TimeCritical,
            4 => Self::MmcssTimeCritical,
            _ => Self::Unknown(v),
        }
    }
}

impl From<AudioThreadPriority> for i32 {
    fn from(v: AudioThreadPriority) -> Self {
        match v {
            AudioThreadPriority::AsioDefault => -1,
            AudioThreadPriority::Normal => 0,
            AudioThreadPriority::AboveNormal => 1,
            AudioThreadPriority::Highest => 2,
            AudioThreadPriority::TimeCritical => 3,
            AudioThreadPriority::MmcssTimeCritical => 4,
            AudioThreadPriority::Unknown(n) => n,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// StartupProjectMode
// ────────────────────────────────────────────────────────────────────────────

/// Which project REAPER opens on startup.
///
/// Maps to REAPER's `loadlastproj` INI key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StartupProjectMode {
    /// Re-open the last active project.
    LastActiveProject,
    /// Re-open all last project tabs.
    LastProjectTabs,
    /// Open a new project, ignoring the default template.
    NewProjectIgnoreTemplate,
    /// Open a new project.
    NewProject,
    /// Prompt to choose.
    Prompt,
    /// A value not recognized by this crate.
    Unknown(u32),
}

impl From<u32> for StartupProjectMode {
    fn from(v: u32) -> Self {
        match v {
            16 => Self::LastActiveProject,
            17 => Self::LastProjectTabs,
            18 => Self::NewProjectIgnoreTemplate,
            19 => Self::NewProject,
            20 => Self::Prompt,
            _ => Self::Unknown(v),
        }
    }
}

impl From<StartupProjectMode> for u32 {
    fn from(v: StartupProjectMode) -> Self {
        match v {
            StartupProjectMode::LastActiveProject => 16,
            StartupProjectMode::LastProjectTabs => 17,
            StartupProjectMode::NewProjectIgnoreTemplate => 18,
            StartupProjectMode::NewProject => 19,
            StartupProjectMode::Prompt => 20,
            StartupProjectMode::Unknown(n) => n,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// ItemMixBehavior
// ────────────────────────────────────────────────────────────────────────────

/// How media items interact when overlapping on a track.
///
/// Maps to REAPER's `itemmixflag` INI key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemMixBehavior {
    /// Enclosed items replace enclosing items (default).
    EnclosedReplace,
    /// Items always mix together.
    AlwaysMix,
    /// Items always replace earlier items.
    AlwaysReplace,
    /// A value not recognized by this crate.
    Unknown(i32),
}

impl From<i32> for ItemMixBehavior {
    fn from(v: i32) -> Self {
        match v {
            0 => Self::EnclosedReplace,
            1 => Self::AlwaysMix,
            2 => Self::AlwaysReplace,
            _ => Self::Unknown(v),
        }
    }
}

impl From<ItemMixBehavior> for i32 {
    fn from(v: ItemMixBehavior) -> Self {
        match v {
            ItemMixBehavior::EnclosedReplace => 0,
            ItemMixBehavior::AlwaysMix => 1,
            ItemMixBehavior::AlwaysReplace => 2,
            ItemMixBehavior::Unknown(n) => n,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// ItemTimeBase
// ────────────────────────────────────────────────────────────────────────────

/// Timebase used for item positions, lengths, and rates.
///
/// Maps to REAPER's `itemtimelock` INI key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemTimeBase {
    /// Positions and lengths are in absolute time.
    Time,
    /// Positions, lengths, and rates follow beats.
    BeatsPositionLengthRate,
    /// Only positions follow beats.
    BeatsPositionOnly,
    /// A value not recognized by this crate.
    Unknown(i32),
}

impl From<i32> for ItemTimeBase {
    fn from(v: i32) -> Self {
        match v {
            0 => Self::Time,
            1 => Self::BeatsPositionLengthRate,
            2 => Self::BeatsPositionOnly,
            _ => Self::Unknown(v),
        }
    }
}

impl From<ItemTimeBase> for i32 {
    fn from(v: ItemTimeBase) -> Self {
        match v {
            ItemTimeBase::Time => 0,
            ItemTimeBase::BeatsPositionLengthRate => 1,
            ItemTimeBase::BeatsPositionOnly => 2,
            ItemTimeBase::Unknown(n) => n,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// TrackMixBitDepth
// ────────────────────────────────────────────────────────────────────────────

/// Internal audio mixing precision for a project.
///
/// Maps to REAPER's `projintmix` INI key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackMixBitDepth {
    /// 64-bit floating point (default).
    Float64,
    /// 32-bit floating point.
    Float32,
    /// 39-bit integer (custom high-precision integer mode).
    Int39,
    /// 24-bit integer.
    Int24,
    /// 16-bit integer.
    Int16,
    /// 12-bit integer.
    Int12,
    /// 8-bit integer.
    Int8,
    /// A value not recognized by this crate.
    Unknown(i32),
}

impl From<i32> for TrackMixBitDepth {
    fn from(v: i32) -> Self {
        match v {
            0 => Self::Float64,
            1 => Self::Float32,
            2 => Self::Int39,
            3 => Self::Int24,
            4 => Self::Int16,
            5 => Self::Int12,
            6 => Self::Int8,
            _ => Self::Unknown(v),
        }
    }
}

impl From<TrackMixBitDepth> for i32 {
    fn from(v: TrackMixBitDepth) -> Self {
        match v {
            TrackMixBitDepth::Float64 => 0,
            TrackMixBitDepth::Float32 => 1,
            TrackMixBitDepth::Int39 => 2,
            TrackMixBitDepth::Int24 => 3,
            TrackMixBitDepth::Int16 => 4,
            TrackMixBitDepth::Int12 => 5,
            TrackMixBitDepth::Int8 => 6,
            TrackMixBitDepth::Unknown(n) => n,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// GroupDisplayMode
// ────────────────────────────────────────────────────────────────────────────

/// How track grouping is visually indicated in the TCP/MCP.
///
/// Maps to REAPER's `groupdispmode` INI key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GroupDisplayMode {
    /// Show grouping as colored ribbons.
    Ribbons,
    /// Show grouping as lines on track edges.
    LinesOnEdge,
    /// No grouping indicator.
    None,
    /// A value not recognized by this crate.
    Unknown(u32),
}

impl From<u32> for GroupDisplayMode {
    fn from(v: u32) -> Self {
        match v {
            0 => Self::Ribbons,
            1 => Self::LinesOnEdge,
            2 => Self::None,
            _ => Self::Unknown(v),
        }
    }
}

impl From<GroupDisplayMode> for u32 {
    fn from(v: GroupDisplayMode) -> Self {
        match v {
            GroupDisplayMode::Ribbons => 0,
            GroupDisplayMode::LinesOnEdge => 1,
            GroupDisplayMode::None => 2,
            GroupDisplayMode::Unknown(n) => n,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// PanDisplayMode
// ────────────────────────────────────────────────────────────────────────────

/// How pan values are displayed on faders.
///
/// Maps to REAPER's `pandispmode` INI key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PanDisplayMode {
    /// Display as 100%L .. 100%R (default).
    FullRange,
    /// Display as -90° .. +90°.
    NinetyDegrees,
    /// A value not recognized by this crate.
    Unknown(u32),
}

impl From<u32> for PanDisplayMode {
    fn from(v: u32) -> Self {
        match v {
            0 => Self::FullRange,
            1 => Self::NinetyDegrees,
            _ => Self::Unknown(v),
        }
    }
}

impl From<PanDisplayMode> for u32 {
    fn from(v: PanDisplayMode) -> Self {
        match v {
            PanDisplayMode::FullRange => 0,
            PanDisplayMode::NinetyDegrees => 1,
            PanDisplayMode::Unknown(n) => n,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// PanLawTaper
// ────────────────────────────────────────────────────────────────────────────

/// Taper curve used for the pan law above −3 dB.
///
/// Maps to REAPER's `panlawflags` INI key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PanLawTaper {
    /// Since taper (default for values near 0).
    SineTaper,
    /// Linear taper.
    LinearTaper,
    /// Hybrid taper (REAPER default).
    HybridTaper,
    /// A value not recognized by this crate.
    Unknown(i32),
}

impl From<i32> for PanLawTaper {
    fn from(v: i32) -> Self {
        match v {
            0 => Self::SineTaper,
            2 => Self::LinearTaper,
            3 => Self::HybridTaper,
            _ => Self::Unknown(v),
        }
    }
}

impl From<PanLawTaper> for i32 {
    fn from(v: PanLawTaper) -> Self {
        match v {
            PanLawTaper::SineTaper => 0,
            PanLawTaper::LinearTaper => 2,
            PanLawTaper::HybridTaper => 3,
            PanLawTaper::Unknown(n) => n,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── TimeDisplayFormat ─────────────────────────────────────────────────

    #[test]
    fn time_display_format_roundtrip() {
        for (n, expected) in [
            (0, TimeDisplayFormat::MinsSecs),
            (1, TimeDisplayFormat::MeasuresBeats),
            (2, TimeDisplayFormat::Seconds),
            (3, TimeDisplayFormat::Samples),
            (4, TimeDisplayFormat::HmsFrames),
            (5, TimeDisplayFormat::AbsoluteFrames),
        ] {
            assert_eq!(TimeDisplayFormat::from(n), expected);
            assert_eq!(i32::from(expected), n);
        }
    }

    #[test]
    fn time_display_format_unknown() {
        assert_eq!(TimeDisplayFormat::from(99), TimeDisplayFormat::Unknown(99));
        assert_eq!(i32::from(TimeDisplayFormat::Unknown(99)), 99);
    }

    // ── RippleMode ────────────────────────────────────────────────────────

    #[test]
    fn ripple_mode_roundtrip() {
        for (n, expected) in [
            (0, RippleMode::Off),
            (1, RippleMode::PerTrack),
            (2, RippleMode::AllTracks),
        ] {
            assert_eq!(RippleMode::from(n), expected);
            assert_eq!(i32::from(expected), n);
        }
    }

    #[test]
    fn ripple_mode_unknown() {
        assert_eq!(RippleMode::from(7), RippleMode::Unknown(7));
        assert_eq!(i32::from(RippleMode::Unknown(7)), 7);
    }

    // ── PanMode ───────────────────────────────────────────────────────────

    #[test]
    fn pan_mode_roundtrip() {
        for (n, expected) in [
            (0, PanMode::Classic),
            (1, PanMode::Balanced),
            (3, PanMode::StereoPan),
            (5, PanMode::DualPan),
            (6, PanMode::MidiPan),
        ] {
            assert_eq!(PanMode::from(n), expected);
            assert_eq!(i32::from(expected), n);
        }
    }

    #[test]
    fn pan_mode_unknown() {
        assert_eq!(PanMode::from(42), PanMode::Unknown(42));
        assert_eq!(i32::from(PanMode::Unknown(42)), 42);
    }

    // ── AutomationMode ────────────────────────────────────────────────────

    #[test]
    fn automation_mode_roundtrip() {
        for (n, expected) in [
            (0, AutomationMode::TrimRead),
            (1, AutomationMode::Read),
            (2, AutomationMode::Touch),
            (3, AutomationMode::Write),
            (4, AutomationMode::Latch),
            (5, AutomationMode::LatchPreview),
        ] {
            assert_eq!(AutomationMode::from(n), expected);
            assert_eq!(i32::from(expected), n);
            assert_eq!(AutomationMode::from(n as u32), expected);
            assert_eq!(u32::from(expected), n as u32);
        }
    }

    #[test]
    fn automation_mode_unknown() {
        assert_eq!(AutomationMode::from(10), AutomationMode::Unknown(10));
        assert_eq!(i32::from(AutomationMode::Unknown(10)), 10);
    }

    // ── ResampleQuality ───────────────────────────────────────────────────

    #[test]
    fn resample_quality_roundtrip() {
        for (n, expected) in [
            (0, ResampleQuality::Lowest),
            (1, ResampleQuality::Low),
            (2, ResampleQuality::Medium),
            (3, ResampleQuality::MediumHigh),
            (4, ResampleQuality::High),
            (5, ResampleQuality::VeryHigh),
            (6, ResampleQuality::Highest),
            (7, ResampleQuality::Best),
        ] {
            assert_eq!(ResampleQuality::from(n), expected);
            assert_eq!(i32::from(expected), n);
        }
    }

    #[test]
    fn resample_quality_unknown() {
        assert_eq!(ResampleQuality::from(255), ResampleQuality::Unknown(255));
        assert_eq!(i32::from(ResampleQuality::Unknown(255)), 255);
    }

    // ── FadeCurveType ─────────────────────────────────────────────────────

    #[test]
    fn fade_curve_type_roundtrip() {
        for (n, expected) in [
            (0u32, FadeCurveType::Linear),
            (1, FadeCurveType::Square),
            (2, FadeCurveType::SlowStartEnd),
            (3, FadeCurveType::FastStart),
            (4, FadeCurveType::FastEnd),
            (5, FadeCurveType::Bezier),
        ] {
            assert_eq!(FadeCurveType::from(n), expected);
            assert_eq!(u32::from(expected), n);
        }
    }

    #[test]
    fn fade_curve_type_unknown() {
        assert_eq!(FadeCurveType::from(99u32), FadeCurveType::Unknown(99));
        assert_eq!(u32::from(FadeCurveType::Unknown(99)), 99u32);
    }

    // ── DiskReadMode ──────────────────────────────────────────────────────

    #[test]
    fn disk_read_mode_roundtrip() {
        for (n, expected) in [
            (0u32, DiskReadMode::AsyncUnbuffered),
            (1, DiskReadMode::AsyncBuffered),
            (2, DiskReadMode::Sync),
        ] {
            assert_eq!(DiskReadMode::from(n), expected);
            assert_eq!(u32::from(expected), n);
        }
        assert_eq!(DiskReadMode::from(99u32), DiskReadMode::Unknown(99));
    }

    // ── DiskWriteMode ─────────────────────────────────────────────────────

    #[test]
    fn disk_write_mode_roundtrip() {
        for (n, expected) in [
            (0u32, DiskWriteMode::Async),
            (1, DiskWriteMode::Sync),
            (2, DiskWriteMode::AsyncWriteThrough),
        ] {
            assert_eq!(DiskWriteMode::from(n), expected);
            assert_eq!(u32::from(expected), n);
        }
        assert_eq!(DiskWriteMode::from(99u32), DiskWriteMode::Unknown(99));
    }

    // ── AutoSaveMode ──────────────────────────────────────────────────────

    #[test]
    fn auto_save_mode_roundtrip() {
        for (n, expected) in [
            (0u32, AutoSaveMode::WhenNotRecording),
            (1, AutoSaveMode::WhenStopped),
            (2, AutoSaveMode::AnyTime),
        ] {
            assert_eq!(AutoSaveMode::from(n), expected);
            assert_eq!(u32::from(expected), n);
        }
        assert_eq!(AutoSaveMode::from(99u32), AutoSaveMode::Unknown(99));
    }

    // ── EnvelopeTrimMode ──────────────────────────────────────────────────

    #[test]
    fn envelope_trim_mode_roundtrip() {
        for (n, expected) in [
            (0u32, EnvelopeTrimMode::Always),
            (1, EnvelopeTrimMode::InReadWrite),
            (2, EnvelopeTrimMode::Never),
        ] {
            assert_eq!(EnvelopeTrimMode::from(n), expected);
            assert_eq!(u32::from(expected), n);
        }
        assert_eq!(EnvelopeTrimMode::from(99u32), EnvelopeTrimMode::Unknown(99));
    }

    // ── VstBridgeMode ─────────────────────────────────────────────────────

    #[test]
    fn vst_bridge_mode_roundtrip() {
        for (n, expected) in [
            (0u32, VstBridgeMode::Auto),
            (1, VstBridgeMode::SeparateProcess),
            (2, VstBridgeMode::DedicatedProcess),
            (3, VstBridgeMode::NativeOnly),
        ] {
            assert_eq!(VstBridgeMode::from(n), expected);
            assert_eq!(u32::from(expected), n);
        }
        assert_eq!(VstBridgeMode::from(99u32), VstBridgeMode::Unknown(99));
    }

    // ── AutoMuteMode ──────────────────────────────────────────────────────

    #[test]
    fn auto_mute_mode_roundtrip() {
        for (n, expected) in [
            (0u32, AutoMuteMode::Off),
            (1, AutoMuteMode::MuteMaster),
            (2, AutoMuteMode::MuteAny),
        ] {
            assert_eq!(AutoMuteMode::from(n), expected);
            assert_eq!(u32::from(expected), n);
        }
        assert_eq!(AutoMuteMode::from(99u32), AutoMuteMode::Unknown(99));
    }

    // ── ZLayer ────────────────────────────────────────────────────────────

    #[test]
    fn z_layer_roundtrip() {
        for (n, expected) in [
            (0u32, ZLayer::OverItems),
            (1, ZLayer::ThroughItems),
            (2, ZLayer::UnderItems),
        ] {
            assert_eq!(ZLayer::from(n), expected);
            assert_eq!(u32::from(expected), n);
        }
        assert_eq!(ZLayer::from(99u32), ZLayer::Unknown(99));
    }

    // ── HelpDisplay ───────────────────────────────────────────────────────

    #[test]
    fn help_display_roundtrip() {
        for (n, expected) in [
            (0u32, HelpDisplay::None),
            (1, HelpDisplay::Tips),
            (2, HelpDisplay::TrackItemCount),
            (3, HelpDisplay::SelectedDetails),
        ] {
            assert_eq!(HelpDisplay::from(n), expected);
            assert_eq!(u32::from(expected), n);
        }
        assert_eq!(HelpDisplay::from(99u32), HelpDisplay::Unknown(99));
    }

    // ── ModalWindowPosition ───────────────────────────────────────────────

    #[test]
    fn modal_window_position_roundtrip() {
        for (n, expected) in [
            (0u32, ModalWindowPosition::LastPosition),
            (1, ModalWindowPosition::CenterCurrentScreen),
            (2, ModalWindowPosition::CenterMouseCursor),
            (3, ModalWindowPosition::OsPositioning),
        ] {
            assert_eq!(ModalWindowPosition::from(n), expected);
            assert_eq!(u32::from(expected), n);
        }
        assert_eq!(
            ModalWindowPosition::from(99u32),
            ModalWindowPosition::Unknown(99)
        );
    }

    // ── ItemVolumeHandlePosition ──────────────────────────────────────────

    #[test]
    fn item_volume_handle_position_roundtrip() {
        for (n, expected) in [
            (0u32, ItemVolumeHandlePosition::AtTop),
            (1, ItemVolumeHandlePosition::AtCenter),
        ] {
            assert_eq!(ItemVolumeHandlePosition::from(n), expected);
            assert_eq!(u32::from(expected), n);
        }
        assert_eq!(
            ItemVolumeHandlePosition::from(99u32),
            ItemVolumeHandlePosition::Unknown(99)
        );
    }

    // ── InsertMasterTrackMode ─────────────────────────────────────────────

    #[test]
    fn insert_master_track_mode_roundtrip() {
        for (n, expected) in [
            (0u32, InsertMasterTrackMode::OneTrack),
            (1, InsertMasterTrackMode::AcrossTracks),
            (2, InsertMasterTrackMode::Auto),
            (3, InsertMasterTrackMode::Prompt),
        ] {
            assert_eq!(InsertMasterTrackMode::from(n), expected);
            assert_eq!(u32::from(expected), n);
        }
        assert_eq!(
            InsertMasterTrackMode::from(99u32),
            InsertMasterTrackMode::Unknown(99)
        );
    }

    // ── AcidImportMode ────────────────────────────────────────────────────

    #[test]
    fn acid_import_mode_roundtrip() {
        for (n, expected) in [
            (0u32, AcidImportMode::AdjustToProject),
            (1, AcidImportMode::SourceTempo),
            (2, AcidImportMode::AlwaysPrompt),
        ] {
            assert_eq!(AcidImportMode::from(n), expected);
            assert_eq!(u32::from(expected), n);
        }
        assert_eq!(AcidImportMode::from(99u32), AcidImportMode::Unknown(99));
    }

    // ── AudioThreadPriority ───────────────────────────────────────────────

    #[test]
    fn audio_thread_priority_roundtrip() {
        for (n, expected) in [
            (-1i32, AudioThreadPriority::AsioDefault),
            (0, AudioThreadPriority::Normal),
            (1, AudioThreadPriority::AboveNormal),
            (2, AudioThreadPriority::Highest),
            (3, AudioThreadPriority::TimeCritical),
            (4, AudioThreadPriority::MmcssTimeCritical),
        ] {
            assert_eq!(AudioThreadPriority::from(n), expected);
            assert_eq!(i32::from(expected), n);
        }
        assert_eq!(
            AudioThreadPriority::from(99),
            AudioThreadPriority::Unknown(99)
        );
    }

    // ── StartupProjectMode ────────────────────────────────────────────────

    #[test]
    fn startup_project_mode_roundtrip() {
        for (n, expected) in [
            (16u32, StartupProjectMode::LastActiveProject),
            (17, StartupProjectMode::LastProjectTabs),
            (18, StartupProjectMode::NewProjectIgnoreTemplate),
            (19, StartupProjectMode::NewProject),
            (20, StartupProjectMode::Prompt),
        ] {
            assert_eq!(StartupProjectMode::from(n), expected);
            assert_eq!(u32::from(expected), n);
        }
        assert_eq!(
            StartupProjectMode::from(0u32),
            StartupProjectMode::Unknown(0)
        );
    }

    // ── PanLawTaper ───────────────────────────────────────────────────────

    #[test]
    fn pan_law_taper_roundtrip() {
        for (n, expected) in [
            (0i32, PanLawTaper::SineTaper),
            (2, PanLawTaper::LinearTaper),
            (3, PanLawTaper::HybridTaper),
        ] {
            assert_eq!(PanLawTaper::from(n), expected);
            assert_eq!(i32::from(expected), n);
        }
        assert_eq!(PanLawTaper::from(1i32), PanLawTaper::Unknown(1));
        assert_eq!(PanLawTaper::from(99i32), PanLawTaper::Unknown(99));
    }

    // ── GroupDisplayMode ──────────────────────────────────────────────────

    #[test]
    fn group_display_mode_roundtrip() {
        for (n, expected) in [
            (0u32, GroupDisplayMode::Ribbons),
            (1, GroupDisplayMode::LinesOnEdge),
            (2, GroupDisplayMode::None),
        ] {
            assert_eq!(GroupDisplayMode::from(n), expected);
            assert_eq!(u32::from(expected), n);
        }
        assert_eq!(GroupDisplayMode::from(99u32), GroupDisplayMode::Unknown(99));
    }

    // ── PanDisplayMode ────────────────────────────────────────────────────

    #[test]
    fn pan_display_mode_roundtrip() {
        for (n, expected) in [
            (0u32, PanDisplayMode::FullRange),
            (1, PanDisplayMode::NinetyDegrees),
        ] {
            assert_eq!(PanDisplayMode::from(n), expected);
            assert_eq!(u32::from(expected), n);
        }
        assert_eq!(PanDisplayMode::from(99u32), PanDisplayMode::Unknown(99));
    }

    // ── ItemMixBehavior ───────────────────────────────────────────────────

    #[test]
    fn item_mix_behavior_roundtrip() {
        for (n, expected) in [
            (0i32, ItemMixBehavior::EnclosedReplace),
            (1, ItemMixBehavior::AlwaysMix),
            (2, ItemMixBehavior::AlwaysReplace),
        ] {
            assert_eq!(ItemMixBehavior::from(n), expected);
            assert_eq!(i32::from(expected), n);
        }
        assert_eq!(ItemMixBehavior::from(99), ItemMixBehavior::Unknown(99));
    }

    // ── ItemTimeBase ──────────────────────────────────────────────────────

    #[test]
    fn item_time_base_roundtrip() {
        for (n, expected) in [
            (0i32, ItemTimeBase::Time),
            (1, ItemTimeBase::BeatsPositionLengthRate),
            (2, ItemTimeBase::BeatsPositionOnly),
        ] {
            assert_eq!(ItemTimeBase::from(n), expected);
            assert_eq!(i32::from(expected), n);
        }
        assert_eq!(ItemTimeBase::from(99), ItemTimeBase::Unknown(99));
    }

    // ── TrackMixBitDepth ──────────────────────────────────────────────────

    #[test]
    fn track_mix_bit_depth_roundtrip() {
        for (n, expected) in [
            (0i32, TrackMixBitDepth::Float64),
            (1, TrackMixBitDepth::Float32),
            (2, TrackMixBitDepth::Int39),
            (3, TrackMixBitDepth::Int24),
            (4, TrackMixBitDepth::Int16),
            (5, TrackMixBitDepth::Int12),
            (6, TrackMixBitDepth::Int8),
        ] {
            assert_eq!(TrackMixBitDepth::from(n), expected);
            assert_eq!(i32::from(expected), n);
        }
        assert_eq!(TrackMixBitDepth::from(99i32), TrackMixBitDepth::Unknown(99));
        assert_eq!(i32::from(TrackMixBitDepth::Unknown(99)), 99);
    }
}
