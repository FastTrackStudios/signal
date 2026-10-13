//! Peak metering service — track meters + take waveform peak data.

use super::{MeterFrame, TakePeakData, TrackPeak};
use crate::item::{ItemRef, TakeRef};
use crate::project::ProjectContext;
use crate::track::TrackRef;

#[architect::rpc]
pub trait Peaks {
    /// Current peak level for a track channel. Peak and peak-hold
    /// values in dB (0.0 = full scale, negative = below).
    fn track_peak(&self, project: ProjectContext, track: TrackRef, channel: u32) -> TrackPeak;

    /// Waveform peak data for a take. `block_size` controls the
    /// resolution: larger = fewer peaks = faster but less detailed.
    /// Typical values 1024–4096 samples per peak.
    fn take_peaks(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        block_size: u32,
    ) -> TakePeakData;

    /// Live per-track meter frames (~30 Hz) for every project with an
    /// attached audio engine, as they happen. Levels are linear `0..1`
    /// (peak + peak-hold per channel); subscribers filter by
    /// `project_guid` client-side and convert to dB for display.
    /// Replaces polling `track_peak` per track per frame — one frame
    /// carries the whole mixer.
    #[subscribe]
    fn meters(&self) -> MeterFrame;
}
