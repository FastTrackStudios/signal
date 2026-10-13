//! Audio accessor data types

use facet::Facet;

/// Request parameters for reading samples from an audio accessor.
#[derive(Clone, Debug, Facet)]
pub struct GetSamplesRequest {
    /// The accessor handle ID (from create_track_accessor / create_take_accessor)
    pub accessor_id: String,
    /// Desired sample rate for the output
    pub sample_rate: f64,
    /// Number of channels to read
    pub num_channels: u32,
    /// Start time in seconds
    pub start_time: f64,
    /// Number of samples per channel to read
    pub num_samples: u32,
}

/// Interleaved sample data read from an audio accessor.
#[derive(Clone, Debug, Facet)]
pub struct AudioSampleData {
    /// Interleaved samples: [ch0_s0, ch1_s0, ch0_s1, ch1_s1, ...]
    pub samples: Vec<f64>,
    /// Sample rate of the data
    pub sample_rate: f64,
    /// Number of channels
    pub num_channels: u32,
    /// Number of samples per channel
    pub num_samples: u32,
}

impl Default for AudioSampleData {
    fn default() -> Self {
        Self {
            samples: Vec::new(),
            sample_rate: 44100.0,
            num_channels: 2,
            num_samples: 0,
        }
    }
}
