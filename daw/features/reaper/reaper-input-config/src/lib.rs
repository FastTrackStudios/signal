//! The canonical keybind profiles, embedded as bytes.
//!
//! `build.rs` walks `../reaper-input/config/config` and generates the
//! statics below. Consumers read the real shipped configuration without
//! linking `reaper-input` itself — which pulls the REAPER FFI and does not
//! build for wasm. The website's `/input` tutorial is why this exists.

#![no_std]

include!(concat!(env!("OUT_DIR"), "/input_profiles.rs"));
