//! Peak metering and waveform data types

use facet::Facet;

/// Peak data for a take waveform
#[derive(Clone, Debug, Facet)]
pub struct TakePeakData {
    /// Sample rate of the peak data
    pub sample_rate: f64,
    /// Number of channels
    pub num_channels: u32,
    /// Peak values per channel (min/max pairs)
    /// Format: [ch0_min, ch0_max, ch1_min, ch1_max, ...]
    pub peaks: Vec<f64>,
    /// Number of samples per peak block
    pub samples_per_peak: u32,
}

/// One track's levels inside a [`MeterFrame`] — linear `0..1`
/// magnitudes (NOT dB; convert client-side, see
/// `daw_standalone::metering::linear_to_db` for the reference curve).
/// Instantaneous block peak + decaying peak-hold, per channel.
// `PartialEq` so a UI can memo on it: a meter frame arrives thirty times a
// second and most strips are unchanged in most frames, so "did this track's
// levels actually move?" is the question that decides whether to re-render.
#[derive(Clone, Copy, Debug, Default, PartialEq, Facet)]
pub struct TrackLevels {
    /// Instantaneous block peak, left channel (linear `0..1`).
    pub peak_left: f32,
    /// Instantaneous block peak, right channel (linear `0..1`).
    pub peak_right: f32,
    /// Decaying peak-hold, left channel (linear `0..1`).
    pub hold_left: f32,
    /// Decaying peak-hold, right channel (linear `0..1`).
    pub hold_right: f32,
}

/// One metering frame for a whole project, published ~30 Hz by the
/// backend's meter pump while an audio engine is attached (see
/// `Peaks::meters`). `tracks[i]` is the levels for project track
/// index `i` — the same order `Tracks::all` returns.
#[derive(Clone, Debug, Facet)]
pub struct MeterFrame {
    /// GUID of the project these levels belong to. Subscribers filter
    /// client-side (the stream is argless on the wire).
    pub project_guid: String,
    /// Per-track levels, indexed by project track index.
    pub tracks: Vec<TrackLevels>,
}

// Trivial Reborrow impl for the owned stream type — lets
// `SelfRef<MeterFrame>::get()` hand subscribers `&MeterFrame`. Safe
// because the type has no borrowed lifetimes.
#[cfg(feature = "vox")]
#[allow(unsafe_code)]
mod reborrow_impls {
    use super::MeterFrame;
    unsafe impl vox_types::Reborrow for MeterFrame {
        type Ref<'a> = MeterFrame;
    }
}

/// Track peak meter reading
#[derive(Clone, Copy, Debug, Facet)]
pub struct TrackPeak {
    /// Peak level in dB (negative values, 0.0 = full scale)
    pub peak_db: f64,
    /// Peak hold level in dB
    pub peak_hold_db: f64,
}

impl Default for TakePeakData {
    fn default() -> Self {
        Self {
            sample_rate: 44100.0,
            num_channels: 2,
            peaks: Vec::new(),
            samples_per_peak: 1024,
        }
    }
}

impl Default for TrackPeak {
    fn default() -> Self {
        Self {
            peak_db: -150.0, // Silence
            peak_hold_db: -150.0,
        }
    }
}
