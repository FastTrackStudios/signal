//! The drum rig on the shared performance surface ([`RigPerform`]), so the
//! screens every rig shares play the drums too:
//!
//! - a **preset** is a kit, and its **variations** the kit as it loads
//!   (`Default`) and the mixes it can take;
//! - each kit piece (kick, snare, …) is a **module**, its presets the
//!   library's instruments of that kind;
//! - **profiles, songs and setlists** are the drums' own, kept apart from
//!   the guitar's in `<config>/signal/drums/perform/`
//!   ([`PerformStore`]); a first start seeds a profile with a stack per kit.
//!
//! Mounted under the drums' scope (`merge_router_scoped("drums", …)`).
//!
//! [`RigPerform`]: signal_rigs_proto::perform::RigPerform

use crate::proto::drum::DrumRig;
use signal_rig_perform_store::{PerformStore, PresetPlayer};
use signal_rigs_proto::perform::{
    Applied, CompositionModel, ModulePick, ModulePresetEntry, PresetEntry, PresetSnapshotEntry,
};

use super::DrumRigBackend;

/// The variation a kit loads as.
const DEFAULT: &str = signal_rig_perform_store::DEFAULT_VARIATION;

impl DrumRigBackend {
    /// The drums' profiles, songs and setlists — opened on first use.
    pub(super) fn store(&self) -> &PerformStore {
        self.inner.perform.get_or_init(|| {
            PerformStore::open(
                signal_rig_perform_store::rig_dir(signal_rigs_proto::Rig::Drums),
                self,
            )
        })
    }

    /// The kits' variations: the kit as it loads, then the mixes.
    fn variations(&self) -> Vec<String> {
        std::iter::once(DEFAULT.to_string())
            // A library mix named like the kit's own load is that, once.
            .chain(
                self.lib()
                    .mixes
                    .iter()
                    .map(|(n, _)| n.clone())
                    .filter(|n| !n.eq_ignore_ascii_case(DEFAULT)),
            )
            .collect()
    }

    /// The variation playing (the kit's own until a mix is picked).
    fn variation(&self) -> String {
        let v = self
            .inner
            .variation
            .lock()
            .map(|v| v.clone())
            .unwrap_or_default();
        if v.is_empty() { DEFAULT.to_string() } else { v }
    }

    /// Play kit `index`'s `variation`: load it if it is not the loaded one,
    /// then put the mix on (or, for `Default`, the kit as it loads).
    fn play_kit(&self, index: usize, variation: &str) -> Applied {
        let kits = DrumRig::kits(self);
        let Some(kit) = kits.get(index) else {
            return Applied::refused(format!("no kit {index}"));
        };
        if variation != DEFAULT && !self.lib().mixes.iter().any(|(n, _)| n == variation) {
            return Applied::refused(format!("no mix {variation:?}"));
        }
        match (kit.loaded, variation == DEFAULT) {
            // The kit as it loads.
            (_, true) => DrumRig::load_kit(self, index as u32),
            // A mix on the loaded kit (the import reloads it clean first).
            (true, false) => DrumRig::import_mm2_mix(self, variation.to_string()),
            // Another kit with a mix: the kit loaded, then the mix — in
            // that order, on one thread, so the mix lands on this kit.
            (false, false) => {
                let (b, mix) = (self.clone(), variation.to_string());
                let _ = signal_rig_host::thread::spawn("drum-load-kit-mix", move || {
                    b.do_load_kit(index);
                    DrumRig::import_mm2_mix(&b, mix);
                });
            }
        }
        if let Ok(mut v) = self.inner.variation.lock() {
            *v = variation.to_string();
        }
        Applied::done()
    }

    /// Tell every screen on the shared surface — and seed the profile once
    /// the kits are known.
    pub(super) fn publish_perform(&self) {
        let store = self.store();
        store.reseed_if_empty(self);
        store.publish(self);
    }
}

impl PresetPlayer for DrumRigBackend {
    fn presets(&self) -> Vec<(String, Vec<String>)> {
        let variations = self.variations();
        DrumRig::kits(self)
            .into_iter()
            .map(|k| (k.name, variations.clone()))
            .collect()
    }

    fn play(&self, preset: &str, variation: &str) -> Applied {
        match DrumRig::kits(self).iter().position(|k| k.name == preset) {
            Some(i) => self.play_kit(
                i,
                if variation.is_empty() {
                    DEFAULT
                } else {
                    variation
                },
            ),
            None => Applied::refused(format!("no kit {preset:?}")),
        }
    }

    fn compositions(&self) -> CompositionModel {
        let kits = DrumRig::kits(self);
        let slots = DrumRig::kit_slots(self);
        // The loaded kit's pieces, each a module playing its instrument.
        let picks: Vec<ModulePick> = slots
            .iter()
            .map(|s| ModulePick {
                module: s.label.clone(),
                preset: s.current_name.clone(),
                snapshot: String::new(),
                blocks: Vec::new(),
            })
            .collect();
        let presets = kits
            .iter()
            .map(|k| PresetEntry {
                name: k.name.clone(),
                snapshots: self
                    .variations()
                    .into_iter()
                    .map(|v| PresetSnapshotEntry {
                        name: v,
                        modules: if k.loaded { picks.clone() } else { Vec::new() },
                        ..PresetSnapshotEntry::default()
                    })
                    .collect(),
                used_by: Vec::new(),
            })
            .collect();
        // Each piece's presets: the library's instruments of its kind.
        let lib = self.lib();
        let modules = slots
            .iter()
            .flat_map(|s| {
                lib.pieces
                    .iter()
                    .filter(|l| l.kind.eq_ignore_ascii_case(&s.kind))
                    .map(|l| ModulePresetEntry {
                        module: s.label.clone(),
                        name: l.name.clone(),
                        ..ModulePresetEntry::default()
                    })
            })
            .collect();
        let loaded = kits.iter().find(|k| k.loaded);
        CompositionModel {
            modules,
            presets,
            active_preset: loaded.map(|k| k.name.clone()).unwrap_or_default(),
            active_snapshot: if loaded.is_some() {
                self.variation()
            } else {
                String::new()
            },
            active_modules: picks,
            block_presets: Vec::new(),
            active_blocks: Vec::new(),
        }
    }
}

signal_rig_perform_store::impl_rig_perform!(DrumRigBackend, |b| b.store());
