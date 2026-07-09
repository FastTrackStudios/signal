//! Control view — re-exports shared audio-gui components used by the compressor.
//!
//! The actual component implementations live in the `audio-gui` crate.
//! This module re-exports them under the names comp-plugin expects,
//! plus the legacy `CompSlider` from fts-plugin-core.

// Re-export shared visualization components
pub use audio_gui::meters::GrMeter;
pub use audio_gui::meters::LevelMeterDb as LevelMeter;
pub use audio_gui::viz::PeakWaveform as WaveformDisplay;
pub use audio_gui::viz::TransferCurve;

// Re-export the legacy CompSlider for backward compat
pub use fts_plugin_core::ui::components::CompSlider;
