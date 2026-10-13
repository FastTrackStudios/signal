//! Audio control widgets.
//!
//! This module provides the core UI widgets for audio parameter control:
//!
//! - [`Knob`] - Rotary control for continuous parameters
//! - [`HSlider`] - Horizontal slider
//! - [`VSlider`] - Vertical slider
//! - [`XYPad`] - Two-dimensional control pad
//! - [`CompressorGraph`] - Compressor transfer curve with interactive controls
//! - [`GateGraph`] - Noise gate transfer curve with Pro-G style controls
//! - [`BlockView`] - Adaptive block rendering with LOD support
//! - [`Pedalboard`] - Pedalboard container for arranging pedals
//! - [`Rack`] - Rack container for arranging rack units

pub mod block_view;
pub mod compressor_graph;
pub mod gate_graph;
pub mod hslider;
pub mod knob;
pub mod mixer;
pub mod vslider;
pub mod xy_pad;

pub use block_view::{BlockView, Pedalboard, Rack};
pub use compressor_graph::{
    CompressorGraph, CompressorMetering, CompressorMode, CompressorParams, CompressorWidget,
    DbRange, DetectionMode, StereoLink,
};
pub use gate_graph::{GateDbRange, GateGraph, GateMetering, GateMode, GateParams};
pub use hslider::{HSlider, SliderVariant};
pub use knob::{Knob, KnobVariant};
pub use mixer::{ChannelFader, ChannelStrip, MixerFolder, MuteButton, RoutingButton, SoloButton};
#[cfg(feature = "hierarchy")]
pub use mixer::{Mixer, MixerStrip};
pub use vslider::VSlider;
pub use xy_pad::XYPad;
