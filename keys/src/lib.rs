//! Live **Keys rig** feature — the vox-served backend + profile/variation
//! logic over the shared keys engine.
//!
//! The engine itself ([`KeysRig`] / [`KeysInstrument`] — a composition-tree
//! preset hosted as one MIDI-driven instrument on daw's audio engine) lives in
//! `sampler::keys_rig` with its `GuitarRig` / `SamplerRig` peers and is
//! re-exported here. This crate adds the keys *product*: the worship profile
//! (engine/layer mixer shape + footswitch stacks), Keyscape preset scanning /
//! normalization, and the [`KeysRigBackend`] wire service.

pub mod proto;
#[cfg(feature = "ui")]
pub mod ui;

// The rig engine (architect::rig, threads, audio) is native-only; the browser
// remote reaches a network engine through `proto`.
#[cfg(not(target_arch = "wasm32"))]
mod backend;
pub mod normalize;
/// Profiles: the engine/layer mixer shape + the footswitch stacks (scenes)
/// that recall it.
///
/// The Worship profile lives here.
pub mod profile;
pub mod user_presets;
pub mod variations;
#[cfg(not(target_arch = "wasm32"))]
pub use backend::KeysRigBackend;
pub use profile::{EngineDef, KeysProfile, KeysStackDef, LayerDef, SceneSlot, worship_profile};
pub use sampler::keys_rig::{KeysInstrument, KeysRig};
