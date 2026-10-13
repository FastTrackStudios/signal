//! Vello scene overlay painters for audio metering.
//!
//! Each painter implements `SceneOverlay` from `nice_plug_dioxus` and reads from
//! the shared `Arc<*State>` types provided by `meter-dsp`.
//!
//! # Usage
//!
//! 1. Create the DSP struct (e.g. [`meter_dsp::spectrum::SpectrumAnalyzer`]).
//! 2. Clone its `.state` arc for the painter.
//! 3. Pass the painter to the Dioxus `SceneOverlay` hook in your plugin editor.
//!
//! # Painters
//!
//! | Module | Painter | DSP State |
//! |---|---|---|
//! | [`spectrum_painter`] | [`spectrum_painter::SpectrumPainter`] | `SpectrumState` |
//! | [`lufs_painter`] | [`lufs_painter::LufsPainter`] | `LufsState` |
//! | [`k_meter_painter`] | [`k_meter_painter::KMeterPainter`] | `KMeterState` |
//! | [`goniometer_painter`] | [`goniometer_painter::GoniometerPainter`] | `PhaseState` |
//! | [`phase_painter`] | [`phase_painter::PhasePainter`] | `PhaseState` |
//! | [`spectrograph_painter`] | [`spectrograph_painter::SpectrographPainter`] | `SpectrumState` |
//! | [`bit_meter_painter`] | [`bit_meter_painter::BitMeterPainter`] | `BitDepthState` |

pub mod bit_meter_painter;
pub mod goniometer_painter;
pub mod k_meter_painter;
pub mod lufs_painter;
pub mod phase_painter;
pub mod spectrograph_painter;
pub mod spectrum_painter;
