//! The e-kit on the shared performance surface: its kits are presets it
//! saves itself — all sixteen pads, their samples and settings — and each
//! kit's variations are snapshots of it. Profiles, songs and setlists are
//! the e-kit's own ([`PerformStore`]), apart from the guitar's and the
//! drums'. Everything lives in `<config>/signal/ekit/perform/`.
//!
//! Mounted under the e-kit's scope (`merge_router_scoped("ekit", …)`).

use crate::proto::Pad;
use facet::Facet;
use signal_rig_host::store::StyxDir;
use signal_rig_perform_store::{DEFAULT_VARIATION, PerformStore, PresetPlayer};
use signal_rigs_proto::perform::Applied;

use super::EkitBackend;

const KITS: &str = "kits.styx";

/// One snapshot of a kit: the space its samples come from, and its pads.
#[derive(Clone, Debug, Default, Facet)]
pub struct KitVariation {
    pub name: String,
    #[facet(default)]
    pub space: String,
    #[facet(default)]
    pub pads: Vec<Pad>,
}

/// A saved kit and its variations.
#[derive(Clone, Debug, Default, Facet)]
pub struct KitDef {
    pub name: String,
    #[facet(default)]
    pub variations: Vec<KitVariation>,
}

#[derive(Clone, Debug, Default, Facet)]
pub struct KitLib {
    #[facet(default)]
    pub kits: Vec<KitDef>,
}

impl EkitBackend {
    fn kits_dir() -> StyxDir {
        StyxDir::new(signal_rig_perform_store::rig_dir(
            signal_rigs_proto::Rig::Ekit,
        ))
    }

    /// The saved kits — a first start saves what is on the pads as `Kit 1`.
    fn kits(&self) -> Vec<KitDef> {
        let mut lib = self
            .inner
            .kits
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if lib.is_none() {
            let mut kits = Self::kits_dir()
                .read::<KitLib>(KITS)
                .map(|l| l.kits)
                .unwrap_or_default();
            if kits.is_empty() {
                kits.push(KitDef {
                    name: "Kit 1".into(),
                    variations: vec![self.snapshot(DEFAULT_VARIATION)],
                });
                Self::kits_dir().write(KITS, &KitLib { kits: kits.clone() });
            }
            *lib = Some(kits);
        }
        lib.clone().unwrap_or_default()
    }

    fn save_kits(&self, kits: Vec<KitDef>) {
        Self::kits_dir().write(KITS, &KitLib { kits: kits.clone() });
        *self
            .inner
            .kits
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(kits);
    }

    /// What is on the pads now, as a variation.
    fn snapshot(&self, name: &str) -> KitVariation {
        let s = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let pads = s
            .pads
            .iter()
            .cloned()
            .map(|p| Pad { peak: 0.0, ..p })
            .collect();
        KitVariation {
            name: name.to_string(),
            space: s.space_name.clone(),
            pads,
        }
    }

    /// Put a variation on the pads: its space, then each pad's sample and
    /// settings.
    fn restore(&self, v: &KitVariation) -> Applied {
        if !v.space.is_empty() && self.load_space(&v.space).is_none() {
            return Applied::refused(format!("no sample space {:?}", v.space));
        }
        let n = {
            let mut s = self
                .inner
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !v.space.is_empty() {
                s.space_name.clone_from(&v.space);
            }
            for (slot, saved) in s.pads.iter_mut().zip(&v.pads) {
                *slot = Pad {
                    index: slot.index,
                    peak: 0.0,
                    ..saved.clone()
                };
            }
            s.pads.len() as u32
        };
        for pad in 0..n {
            self.apply_pad_mix(pad, true);
        }
        self.publish();
        Applied::done()
    }

    /// The e-kit's profiles, songs and setlists — opened on first use.
    pub(super) fn store(&self) -> &PerformStore {
        self.inner.perform.get_or_init(|| {
            PerformStore::open(
                signal_rig_perform_store::rig_dir(signal_rigs_proto::Rig::Ekit),
                self,
            )
        })
    }

    fn set_playing(&self, kit: &str, variation: &str) {
        *self
            .inner
            .playing
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            (kit.to_string(), variation.to_string());
    }
}

impl PresetPlayer for EkitBackend {
    fn presets(&self) -> Vec<(String, Vec<String>)> {
        self.kits()
            .into_iter()
            .map(|k| (k.name, k.variations.into_iter().map(|v| v.name).collect()))
            .collect()
    }

    fn play(&self, preset: &str, variation: &str) -> Applied {
        let kits = self.kits();
        let Some(kit) = kits.iter().find(|k| k.name == preset) else {
            return Applied::refused(format!("no kit {preset:?}"));
        };
        let Some(v) = kit
            .variations
            .iter()
            .find(|v| v.name == variation)
            .or_else(|| kit.variations.first())
        else {
            return Applied::refused(format!("{preset:?} has no variations"));
        };
        let a = self.restore(v);
        if a.ok {
            self.set_playing(preset, &v.name);
        }
        a
    }

    fn compositions(&self) -> signal_rigs_proto::perform::CompositionModel {
        let (kit, variation) = self
            .inner
            .playing
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        signal_rigs_proto::perform::CompositionModel {
            presets: self
                .kits()
                .into_iter()
                .map(|k| signal_rigs_proto::perform::PresetEntry {
                    name: k.name,
                    snapshots: k
                        .variations
                        .into_iter()
                        .map(|v| signal_rigs_proto::perform::PresetSnapshotEntry {
                            name: v.name,
                            ..Default::default()
                        })
                        .collect(),
                    used_by: Vec::new(),
                })
                .collect(),
            active_preset: kit,
            active_snapshot: variation,
            ..Default::default()
        }
    }

    fn save_variation(&self, preset: &str, variation: &str) -> Applied {
        let variation = if variation.trim().is_empty() {
            DEFAULT_VARIATION
        } else {
            variation
        };
        let snap = self.snapshot(variation);
        let mut kits = self.kits();
        match kits.iter_mut().find(|k| k.name == preset) {
            Some(k) => match k.variations.iter_mut().find(|v| v.name == variation) {
                Some(v) => *v = snap,
                None => k.variations.push(snap),
            },
            None => kits.push(KitDef {
                name: preset.to_string(),
                variations: vec![snap],
            }),
        }
        self.save_kits(kits);
        self.set_playing(preset, variation);
        Applied::done()
    }

    fn duplicate_preset(&self, name: &str, new_name: &str) -> Applied {
        let mut kits = self.kits();
        if new_name.trim().is_empty() || kits.iter().any(|k| k.name == new_name) {
            return Applied::refused(format!("{new_name:?} is empty or taken"));
        }
        let kit = match kits.iter().find(|k| k.name == name) {
            Some(k) => KitDef {
                name: new_name.to_string(),
                ..k.clone()
            },
            None => KitDef {
                name: new_name.to_string(),
                variations: vec![self.snapshot(DEFAULT_VARIATION)],
            },
        };
        kits.push(kit);
        self.save_kits(kits);
        Applied::done()
    }

    fn rename_preset(&self, old: &str, new_name: &str) -> Applied {
        let mut kits = self.kits();
        if new_name.trim().is_empty() || kits.iter().any(|k| k.name == new_name) {
            return Applied::refused(format!("{new_name:?} is empty or taken"));
        }
        let Some(k) = kits.iter_mut().find(|k| k.name == old) else {
            return Applied::refused(format!("no kit {old:?}"));
        };
        k.name = new_name.to_string();
        self.save_kits(kits);
        let mut playing = self
            .inner
            .playing
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if playing.0 == old {
            playing.0 = new_name.to_string();
        }
        Applied::done()
    }

    fn delete_preset(&self, name: &str) -> Applied {
        let mut kits = self.kits();
        if kits.len() <= 1 {
            return Applied::refused("the only kit");
        }
        if self
            .inner
            .playing
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .0
            == name
        {
            return Applied::refused("it is playing");
        }
        kits.retain(|k| k.name != name);
        self.save_kits(kits);
        Applied::done()
    }
}

signal_rig_perform_store::impl_rig_perform!(EkitBackend, |b| b.store());
