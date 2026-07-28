//! Shared live-rig wire contract (#65 phase 3).
//!
//! Every rig backend implements [`rig_core::RigCore`] — the surface all five
//! rigs duplicated in their own protos (start / stop / presets / load / MIDI
//! port plumbing). One trait would collide on the engine's merged router
//! (vox method ids hash names only), so each rig mounts it **instance-
//! scoped**: `router.merge_router_scoped("keys", …)` server-side,
//! `architect::scope_client!(client, "keys")` client-side. The per-rig
//! protos keep only what is genuinely that rig's own (tuner, drum maps,
//! layer detail, …).

use facet::Facet;

/// One selectable preset/kit/patch in a rig's library.
#[derive(Clone, Debug, Default, PartialEq, Facet)]
pub struct RigPresetInfo {
    pub name: String,
    /// Whether this entry is the loaded one.
    pub loaded: bool,
}

pub mod rig_core {
    //! `RigCore` → `RigCoreClient` / `RigCoreService`.
    use super::RigPresetInfo;

    #[architect::rpc]
    pub trait RigCore {
        /// Open audio (idempotent; heavy work off-thread).
        fn start(&self);
        /// Close audio.
        fn stop(&self);
        /// Audio device open and processing.
        fn running(&self) -> bool;
        /// The rig's selectable presets (kits / patches / programs).
        fn presets(&self) -> Vec<RigPresetInfo>;
        /// Load presets()[index].
        fn load_preset(&self, index: u32);
        /// Hardware MIDI input ports.
        fn midi_ports(&self) -> Vec<String>;
        /// Select the MIDI input port (empty = omni).
        fn set_midi_port(&self, name: String);
        /// Recent MIDI traffic, rendered for display.
        fn midi_recent(&self) -> Vec<String>;
    }
}

pub use rig_core::prelude::*;
