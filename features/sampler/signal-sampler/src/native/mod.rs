//! Built-in **native DSP** blocks — the `Native` implementations of the
//! composition tree's block types, grown one type at a time (roadmap §1).
//!
//! Each block is a struct implementing [`PluginInstance`], registered in
//! [`native_dsp_available`](crate::rig::native_dsp_available) and dispatched in
//! [`build_node_backend`](crate::node_render::build_node_backend). Generators
//! (Oscillator, Tonewheel, …) ignore their audio input and render from MIDI;
//! processors (Filter, Amp, FX) transform their input.
//!
//! This module is the seed of the eventual `daw-builtin-fx` crate — keep it
//! free of platform I/O and hot-path allocation so it can move wholesale.
//!
//! [`PluginInstance`]: signal_plugin_host::PluginInstance

mod adsr;
mod amp;
mod arp;
mod control;
mod filter;
mod freq_shifter;
mod modal;
mod registry;
mod waveshaper;
mod wavetable;
mod wurli;

pub use adsr::{Adsr, AdsrParams};
pub use amp::NativeAmp;
pub use arp::{ArpEngine, ArpStep};
pub use control::{ControlEnv, ControlLfo, ControlSource, LfoWave, MidiMod, MidiSource, ModSource};
pub use filter::{FilterCharacter, FilterMode, Ladder, NativeFilter, Svf};
pub use freq_shifter::NativeDfs;
pub use modal::NativeModal;
pub use registry::{build_native, native_dsp_available};
pub use waveshaper::NativeWaveshaper;
pub use wavetable::{HarmVoice, NativeWavetable, SynthConfig};
pub use wurli::NativeWurli;
