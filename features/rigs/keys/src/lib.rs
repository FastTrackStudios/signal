//! Live **Keys rig** feature — the vox-served backend + profile/variation
//! logic over the shared keys engine.
//!
//! The engine itself ([`KeysRig`] / [`KeysInstrument`] — a composition-tree
//! preset hosted as one MIDI-driven instrument on daw's audio engine) lives in
//! `signal_sampler::keys_rig` with its `GuitarRig` / `SamplerRig` peers and is
//! re-exported here. This crate adds the keys *product*: the worship profile
//! (engine/layer mixer shape + footswitch stacks), Keyscape preset scanning /
//! normalization, and the [`KeysRigBackend`] wire service.

mod backend;
pub mod normalize;
pub mod variations;
/// Profiles: the engine/layer mixer shape + the footswitch stacks (scenes)
/// that recall it. The Worship profile lives here.
pub mod profile;
pub use backend::KeysRigBackend;
pub use profile::{EngineDef, KeysProfile, KeysStackDef, LayerDef, SceneSlot, worship_profile};
pub use signal_keys_proto as proto;
pub use signal_sampler::keys_rig::{KeysInstrument, KeysRig};
