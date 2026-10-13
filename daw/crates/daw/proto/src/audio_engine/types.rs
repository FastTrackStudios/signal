//! Audio engine data types — latency, device info, engine state.

use facet::Facet;

/// Audio latency information from the audio device.
///
/// Latency values are provided in both samples and seconds. The sample
/// rate is included to allow conversion between the two.
#[derive(Clone, Debug, Default, Facet)]
pub struct AudioLatency {
    pub input_samples: u32,
    pub output_samples: u32,
    /// Output latency in seconds (samples / sample_rate).
    pub output_seconds: f64,
    pub sample_rate: u32,
}

/// Complete audio engine state.
#[derive(Clone, Debug, Default, Facet)]
pub struct AudioEngineState {
    pub is_running: bool,
    pub is_prebuffer: bool,
    pub latency: AudioLatency,
}

/// A single audio input channel on the current audio device.
#[derive(Clone, Debug, Facet)]
pub struct AudioInputChannel {
    /// 0-based channel index (matches REAPER's I_RECINPUT for mono).
    pub index: u32,
    /// Human-readable name from the audio driver.
    pub name: String,
}

/// Summary of the current audio device's input capabilities.
#[derive(Clone, Debug, Default, Facet)]
pub struct AudioInputInfo {
    /// Audio device identifier (e.g. "Galaxy 32").
    pub device_name: String,
    /// All available input channels.
    pub channels: Vec<AudioInputChannel>,
}
