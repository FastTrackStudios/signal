//! Electronic Kit rig wire contract (#77 M3): a grid of pads, each holding
//! one sample drawn from a built sample space, with Atlas/XO kit semantics
//! (per-pad category, locks, kit generation, similarity stepping, morph).

use facet::Facet;

/// One pad's full state.
#[derive(Facet, Clone, Debug, Default)]
pub struct Pad {
    pub index: u32,
    /// The class this pad draws from ("kick", "snare", …). Dropping a
    /// sample of a different class re-assigns it (Atlas rule).
    pub category: String,
    /// Space the current sample came from, and its index within it.
    pub space: String,
    pub item_idx: u32,
    /// Display path of the loaded sample ("" when empty).
    pub path: String,
    /// Keep the SAMPLE through kit generation / morphs.
    pub locked: bool,
    /// Keep the PARAMS when a new sample lands on this pad.
    pub params_locked: bool,
    pub gain_db: f32,
    pub pan: f32,
    /// Semitones, ±24.
    pub pitch: f32,
    /// Amp shaping (ms). `release_ms` 0 = play to the end.
    pub attack_ms: f32,
    pub release_ms: f32,
    pub reverse: bool,
    /// 0 = none, 1..=5 = mute group.
    pub choke_group: u32,
    pub muted: bool,
    pub soloed: bool,
    /// Peak meter for this pad (0..1), most recent pump tick.
    pub peak: f32,
}

#[derive(Facet, Clone, Debug, Default)]
pub struct EkitStatus {
    pub running: bool,
    pub space: String,
    /// Grid dimensions (rows × cols); 4×4 by default.
    pub rows: u32,
    pub cols: u32,
    pub last_error: String,
    /// MIDI note of pad 0; pad N = base + N.
    pub base_note: u32,
}

#[derive(Facet, Clone, Debug)]
#[repr(u8)]
pub enum EkitEvent {
    Status(EkitStatus),
    Pads(Vec<Pad>),
    /// Pad index that just fired (UI flash).
    Hit(u32),
}

pub mod ekit {
    //! `EkitRig` → `EkitRigClient` / `EkitRigService`.
    use super::{EkitEvent, EkitStatus, Pad};

    #[architect::rpc]
    pub trait EkitRig {
        fn start(&self);
        fn stop(&self);
        fn status(&self) -> EkitStatus;
        fn pads(&self) -> Vec<Pad>;
        /// Point the rig at a built space (its samples become the pool).
        fn set_space(&self, space: String);
        /// Play a pad (velocity 0 = note off).
        fn trigger(&self, pad: u32, velocity: u32);
        /// Load a specific space item onto a pad; the pad's category follows
        /// the item's class.
        fn load_item(&self, pad: u32, item_idx: u32);
        /// Re-roll ONE pad from its category (Atlas per-pad Randomize).
        fn randomize_pad(&self, pad: u32);
        /// Step a pad through its similarity list (+1 / -1 …).
        fn step_similar(&self, pad: u32, delta: i32);
        /// Fill every unlocked pad from its own category (Atlas "New Kit").
        fn new_kit(&self);
        /// Step EVERY unlocked pad's similarity list together (XO "Kit
        /// Similarity" — morph the whole kit toward neighbouring sounds).
        fn morph_kit(&self, delta: i32);
        fn set_category(&self, pad: u32, category: String);
        fn set_locked(&self, pad: u32, locked: bool);
        fn set_params_locked(&self, pad: u32, locked: bool);
        fn set_pad_param(&self, pad: u32, param: String, value: f32);
        fn set_muted(&self, pad: u32, muted: bool);
        fn set_soloed(&self, pad: u32, soloed: bool);
        fn midi_ports(&self) -> Vec<String>;
        fn set_midi_port(&self, name: String);
        #[subscribe]
        fn events(&self) -> EkitEvent;
    }
}
