//! Audio visualization widgets — level meters, waveform displays, spectrum.
//!
//! All visualizations take pre-computed data via props. The audio analysis
//! pipeline is external to these components.

mod level_meter;
mod spectrum;
mod waveform;

pub use level_meter::{LevelMeter, LevelMeterOrientation};
pub use spectrum::SpectrumAnalyzer;
pub use waveform::WaveformDisplay;
