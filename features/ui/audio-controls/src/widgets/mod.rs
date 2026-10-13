//! Audio control widgets.
//!
//! This module provides the core UI widgets for audio parameter control:
//!
//! - [`Knob`] - Rotary control for continuous parameters
//! - [`HSlider`] - Horizontal slider
//! - [`VSlider`] - Vertical slider
//! - [`BlockView`] - Adaptive block rendering with LOD support
//! - [`deesser_graph`] - De-esser band graph
//! - [`trace`] - Recorded metering, so a graph moves without live DSP
//! - [`Pedalboard`] - Pedalboard container for arranging pedals
//! - [`Rack`] - Rack container for arranging rack units

pub mod block_view;
pub mod deesser_graph;
pub mod hslider;
pub mod knob;
pub mod mixer;
pub mod trace;
pub mod vslider;

pub use block_view::{BlockView, Pedalboard, Rack};
pub use hslider::{HSlider, SliderVariant};
pub use knob::{Knob, KnobVariant};
pub use mixer::{ChannelFader, ChannelStrip, MixerFolder, MuteButton, RoutingButton, SoloButton};
#[cfg(feature = "hierarchy")]
pub use mixer::{Mixer, MixerStrip};
pub use vslider::VSlider;
