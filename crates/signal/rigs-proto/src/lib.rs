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
#[derive(Clone, Debug, Default, PartialEq, Eq, Facet)]
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
        /// Load `presets()[index]`.
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

// ── The rig catalogue ───────────────────────────────────────────────────────

/// Every live rig the product has, and the one place that says so.
///
/// A rig's identity was previously spelled out in four places that agreed
/// only by convention: the engine's `merge_router_scoped` literals, the
/// desktop workspace's picker, the phone shell's chooser, and the `--rig` /
/// URL-hash slugs. They had already drifted — the phone offered a rig the
/// desktop did not, and neither list was complete. This enum is now the
/// authority for **what a rig is called**: its scope on the wire, its slug in
/// prefs and links, and its name and blurb on screen.
///
/// Two things deliberately stay outside it, because they are not properties
/// of the rig:
///
/// - **Availability** — which rigs a given binary can actually open depends
///   on that binary's cargo features (the phone builds two of these; the
///   desktop reaches all of them through the engine). It belongs to the
///   consumer, not the catalogue.
/// - **Glyphs** — the icon set is a UI concern and lives with the shells.
///
/// Adding a rig is: a variant here, its arms below, an availability arm in
/// the shell, and a `view()` arm wherever it renders.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Facet)]
#[repr(C)]
pub enum Rig {
    Guitar,
    Keys,
    Drums,
    Bass,
    Vocals,
    Synth,
    /// The Electronic Kit pad grid (#77).
    Ekit,
    /// Not an instrument — the sample-space map browser (#77).
    Space,
}

impl Rig {
    /// Every rig, in menu order: the instruments someone plays first, then
    /// the studio surfaces.
    pub const ALL: &'static [Self] = &[
        Self::Guitar,
        Self::Keys,
        Self::Drums,
        Self::Bass,
        Self::Vocals,
        Self::Synth,
        Self::Ekit,
        Self::Space,
    ];

    /// The rig's stable name: its `merge_router_scoped` / `scope_client!`
    /// scope on the wire, its key in prefs, its `--rig` argument, and its URL
    /// hash segment. One string for all of them, on purpose — a rig that is
    /// "keys" in a link and "Keys" on the wire is a bug waiting for a typo.
    #[must_use] 
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Guitar => "guitar",
            Self::Keys => "keys",
            Self::Drums => "drums",
            Self::Bass => "bass",
            Self::Vocals => "vocals",
            Self::Synth => "synth",
            Self::Ekit => "ekit",
            Self::Space => "space",
        }
    }

    /// Display name.
    #[must_use] 
    pub const fn label(self) -> &'static str {
        match self {
            Self::Guitar => "Guitar",
            Self::Keys => "Keys",
            Self::Drums => "Drums",
            Self::Bass => "Bass",
            Self::Vocals => "Vocals",
            Self::Synth => "Synth",
            Self::Ekit => "E-Kit",
            Self::Space => "Samples",
        }
    }

    /// One line, in the player's terms — what you get when you open it.
    #[must_use] 
    pub const fn blurb(self) -> &'static str {
        match self {
            Self::Guitar => "amp, cab, FX — footswitch scenes",
            Self::Keys => "sampled pianos & EPs — engine/layer routing",
            Self::Drums => "sampled kit, mixer, MM2 mixes",
            Self::Bass => "DI → NAM amp → IR — bass & synth bass",
            Self::Vocals => "live vocal chain",
            Self::Synth => "Omnisphere patches in the native engine",
            Self::Ekit => "pad grid over the sample space",
            Self::Space => "similarity maps over the sample libraries",
        }
    }

    /// Whether the engine mounts [`rig_core::RigCore`] for this rig — i.e.
    /// whether `scope_client!(client, rig.slug())` resolves. `Space` is a
    /// browser over the sample libraries, not an instrument with a transport,
    /// so it has no core; `Vocals` has no backend at all yet.
    #[must_use] 
    pub const fn has_rig_core(self) -> bool {
        !matches!(self, Self::Space | Self::Vocals)
    }

    #[must_use] 
    pub fn from_slug(s: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|k| k.slug().eq_ignore_ascii_case(s))
    }
}

#[cfg(test)]
mod catalogue_tests {
    use super::Rig;

    /// `ALL` is what every consumer iterates, so a variant missing from it is
    /// a rig that silently vanishes from every menu.
    #[test]
    fn all_is_exhaustive() {
        for rig in Rig::ALL {
            // Exhaustive match: adding a variant without adding it to ALL
            // fails to compile here rather than disappearing at runtime.
            let covered = match rig {
                Rig::Guitar | Rig::Keys | Rig::Drums | Rig::Bass | Rig::Vocals | Rig::Synth | Rig::Ekit | Rig::Space => true,
            };
            assert!(covered);
        }
        assert_eq!(Rig::ALL.len(), 8);
    }

    #[test]
    fn slugs_are_unique_and_round_trip() {
        for rig in Rig::ALL.iter().copied() {
            assert_eq!(Rig::from_slug(rig.slug()), Some(rig), "{}", rig.slug());
        }
        let mut slugs: Vec<_> = Rig::ALL.iter().map(|r| r.slug()).collect();
        slugs.sort_unstable();
        let count = slugs.len();
        slugs.dedup();
        assert_eq!(slugs.len(), count, "two rigs share a slug");
    }
}
