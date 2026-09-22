//! Presets as compositions of module presets, snapshots all the way down.
//!
//! - A **module preset** is a named sound for one module (Amp, Drive, Time,
//!   Modulation, Dynamics) with **snapshots** — its variations. An Amp
//!   snapshot carries the captures and cabs; a Drive snapshot the pedal in
//!   each slot; every snapshot carries overrides.
//! - A **preset** picks one module snapshot per module, and has snapshots of
//!   its own: "Fender" with Clean, Dry and Verb rather than three presets.
//! - A **patch** plays a preset snapshot, may replace any module's pick, and
//!   adds its own overrides.
//!
//! Overrides layer in that order — module snapshot, preset snapshot, patch —
//! so a later one wins, and anything can bend anything below it.
//!
//! Both libraries are shared by every profile (`modules.styx`,
//! `presets.styx`). [`flatten`] resolves a profile's patches into the fields
//! the chain builders already play — `preset`, `preset2`, `drives`,
//! `overrides` — so the engine gains one concept (per-patch drives) and no
//! second way to build a chain.

use facet::Facet;

use crate::profiles::{DriveSlotDef, ModuleChoiceDef, OverrideDef, PatchDef, PresetDef, ProfileDef};

/// The module-preset library's file in the rig directory.
pub const MODULES_FILE: &str = "modules.styx";
/// The preset library's file.
pub const PRESETS_FILE: &str = "presets.styx";

/// The modules a preset composes, in signal order.
pub const MODULES: [&str; 5] = ["Dynamics", "Drive", "Amp", "Modulation", "Time"];

/// One variation of a module preset.
#[derive(Clone, Debug, Default, Facet)]
pub struct ModuleSnapshotDef {
    pub name: String,
    /// Amp: the capture in Amp L, and the IR after it (empty for a full rig).
    #[facet(default)]
    pub nam: String,
    #[facet(default)]
    pub cab: String,
    /// Amp: a second amp in Amp R, with its cab. Empty leaves the slot off.
    #[facet(default)]
    pub nam2: String,
    #[facet(default)]
    pub cab2: String,
    /// Drive: which pedal (and which of its captures) each slot runs.
    #[facet(default)]
    pub drives: Vec<DriveSlotDef>,
    #[facet(default)]
    pub overrides: Vec<OverrideDef>,
}

/// A named sound for one module, with its snapshots.
#[derive(Clone, Debug, Default, Facet)]
pub struct ModulePresetDef {
    pub module: String,
    pub name: String,
    pub snapshots: Vec<ModuleSnapshotDef>,
}

/// One variation of a preset: a module snapshot per module, plus overrides.
#[derive(Clone, Debug, Default, Facet)]
pub struct PresetSnapshotDef {
    pub name: String,
    #[facet(default)]
    pub modules: Vec<ModuleChoiceDef>,
    #[facet(default)]
    pub overrides: Vec<OverrideDef>,
}

/// A preset: a composition of module presets, with snapshots.
#[derive(Clone, Debug, Default, Facet)]
pub struct RigPresetDef {
    pub name: String,
    pub snapshots: Vec<PresetSnapshotDef>,
}

/// `modules.styx`.
#[derive(Clone, Debug, Default, Facet)]
pub struct ModuleLib {
    #[facet(default)]
    pub presets: Vec<ModulePresetDef>,
}

/// `presets.styx`.
#[derive(Clone, Debug, Default, Facet)]
pub struct PresetLib {
    #[facet(default)]
    pub presets: Vec<RigPresetDef>,
}

/// Both libraries, as one value to resolve against.
#[derive(Clone, Debug, Default)]
pub struct Compositions {
    pub modules: Vec<ModulePresetDef>,
    pub presets: Vec<RigPresetDef>,
}

impl Compositions {
    #[must_use]
    pub fn module(&self, module: &str, preset: &str) -> Option<&ModulePresetDef> {
        self.modules
            .iter()
            .find(|m| m.module.eq_ignore_ascii_case(module) && m.name.eq_ignore_ascii_case(preset))
    }

    #[must_use]
    pub fn preset(&self, name: &str) -> Option<&RigPresetDef> {
        self.presets.iter().find(|p| p.name.eq_ignore_ascii_case(name))
    }

    /// Module presets for one module, in library order.
    pub fn modules_of<'a>(&'a self, module: &'a str) -> impl Iterator<Item = &'a ModulePresetDef> + 'a {
        self.modules
            .iter()
            .filter(move |m| m.module.eq_ignore_ascii_case(module))
    }
}

/// A snapshot by name, or the first when the name is empty or unknown —
/// a preset with one variation should not need its snapshot spelled out.
fn snapshot<'a, T>(list: &'a [T], name: &str, name_of: impl Fn(&T) -> &str) -> Option<&'a T> {
    list.iter()
        .find(|s| !name.is_empty() && name_of(s).eq_ignore_ascii_case(name))
        .or_else(|| list.first())
}

/// The module picks a patch ends up with: its preset snapshot's, with the
/// patch's own picks replacing them module by module.
#[must_use]
pub fn module_picks(comp: &Compositions, patch: &PatchDef) -> Vec<ModuleChoiceDef> {
    let mut picks: Vec<ModuleChoiceDef> = comp
        .preset(&patch.rig_preset)
        .and_then(|p| snapshot(&p.snapshots, &patch.snapshot, |s| &s.name))
        .map(|s| s.modules.clone())
        .unwrap_or_default();
    for own in &patch.modules {
        match picks.iter_mut().find(|p| p.module.eq_ignore_ascii_case(&own.module)) {
            Some(p) => *p = own.clone(),
            None => picks.push(own.clone()),
        }
    }
    picks
}

/// The pool name a synthesised amp preset gets. Distinct from anything a
/// person would type, so it cannot collide with a hand-made pool preset.
#[must_use]
pub fn amp_pool_name(preset: &str, snapshot: &str, slot: &str) -> String {
    format!("{preset} · {snapshot} [{slot}]")
}

/// Resolve every composed patch into the fields the chain builders play.
///
/// Patches with no preset and no module picks pass through untouched, so a
/// profile written before compositions existed builds exactly as before.
#[must_use]
pub fn flatten(def: &ProfileDef, comp: &Compositions) -> ProfileDef {
    let mut out = def.clone();
    let mut synthesised: Vec<PresetDef> = Vec::new();
    for patch in &mut out.patches {
        if patch.rig_preset.is_empty() && patch.modules.is_empty() {
            continue;
        }
        let picks = module_picks(comp, patch);
        let mut overrides: Vec<OverrideDef> = Vec::new();

        // Module snapshots first, in signal order, then anything unknown.
        let ordered = MODULES
            .iter()
            .filter_map(|m| picks.iter().find(|p| p.module.eq_ignore_ascii_case(m)))
            .chain(picks.iter().filter(|p| !MODULES.iter().any(|m| p.module.eq_ignore_ascii_case(m))));
        for pick in ordered {
            let Some(module) = comp.module(&pick.module, &pick.preset) else {
                tracing::warn!(patch = %patch.name, module = %pick.module, preset = %pick.preset, "compose: no such module preset");
                continue;
            };
            let Some(snap) = snapshot(&module.snapshots, &pick.snapshot, |s| &s.name) else {
                continue;
            };
            if pick.module.eq_ignore_ascii_case("Amp") {
                let mut pool = |slot: &str, nam: &str, cab: &str| -> String {
                    if nam.is_empty() {
                        return String::new();
                    }
                    let name = amp_pool_name(&module.name, &snap.name, slot);
                    if !synthesised.iter().any(|p| p.name == name) {
                        synthesised.push(PresetDef {
                            name: name.clone(),
                            nam: nam.to_string(),
                            hash: String::new(),
                            cab: cab.to_string(),
                            cab_hash: String::new(),
                        });
                    }
                    name
                };
                let l = pool("L", &snap.nam, &snap.cab);
                if !l.is_empty() {
                    patch.preset = l;
                }
                patch.preset2 = pool("R", &snap.nam2, &snap.cab2);
            }
            for d in &snap.drives {
                match patch.drives.iter_mut().find(|x| x.block.eq_ignore_ascii_case(&d.block)) {
                    Some(x) => *x = d.clone(),
                    None => patch.drives.push(d.clone()),
                }
            }
            overrides.extend(snap.overrides.iter().cloned());
        }
        if let Some(snap) = comp
            .preset(&patch.rig_preset)
            .and_then(|p| snapshot(&p.snapshots, &patch.snapshot, |s| &s.name))
        {
            overrides.extend(snap.overrides.iter().cloned());
        }
        overrides.append(&mut patch.overrides);
        patch.overrides = overrides;
    }
    out.presets.extend(synthesised);
    out
}

/// A patch's drive slots: the profile's, with the patch's own over them.
#[must_use]
pub fn drives_for(def: &ProfileDef, patch: &PatchDef) -> Vec<DriveSlotDef> {
    let mut out = def.drives.clone();
    for d in &patch.drives {
        match out.iter_mut().find(|x| x.block.eq_ignore_ascii_case(&d.block)) {
            Some(x) => *x = d.clone(),
            None => out.push(d.clone()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiles::{drive_presets, worship_def};

    fn choice(module: &str, preset: &str, snapshot: &str) -> ModuleChoiceDef {
        ModuleChoiceDef {
            module: module.into(),
            preset: preset.into(),
            snapshot: snapshot.into(),
        }
    }

    fn comp() -> Compositions {
        Compositions {
            modules: vec![
                ModulePresetDef {
                    module: "Amp".into(),
                    name: "Deluxe".into(),
                    snapshots: vec![
                        ModuleSnapshotDef {
                            name: "Clean".into(),
                            nam: "/caps/deluxe-clean.nam".into(),
                            cab: "/irs/1x12.wav".into(),
                            overrides: vec![OverrideDef::set("Eq", "Amp EQ", "b2_gain", -1.0)],
                            ..ModuleSnapshotDef::default()
                        },
                        ModuleSnapshotDef {
                            name: "Edge".into(),
                            nam: "/caps/deluxe-edge.nam".into(),
                            nam2: "/caps/ac30.nam".into(),
                            ..ModuleSnapshotDef::default()
                        },
                    ],
                },
                ModulePresetDef {
                    module: "Time".into(),
                    name: "Plate".into(),
                    snapshots: vec![ModuleSnapshotDef {
                        name: "Short".into(),
                        overrides: vec![OverrideDef::set("Time", "VERB 1", "mix", 0.2)],
                        ..ModuleSnapshotDef::default()
                    }],
                },
                ModulePresetDef {
                    module: "Drive".into(),
                    name: "Klon".into(),
                    snapshots: vec![ModuleSnapshotDef {
                        name: "On".into(),
                        drives: vec![DriveSlotDef {
                            block: "Drive 1".into(),
                            preset: "JHS Morning Glory".into(),
                            option: 2,
                        }],
                        overrides: vec![OverrideDef::bypass("Drive", "Drive 1", false)],
                        ..ModuleSnapshotDef::default()
                    }],
                },
            ],
            presets: vec![RigPresetDef {
                name: "Fender".into(),
                snapshots: vec![
                    PresetSnapshotDef {
                        name: "Clean".into(),
                        modules: vec![choice("Amp", "Deluxe", "Clean"), choice("Time", "Plate", "Short")],
                        overrides: vec![OverrideDef::set("Time", "VERB 1", "mix", 0.3)],
                    },
                    PresetSnapshotDef {
                        name: "Edge".into(),
                        modules: vec![choice("Amp", "Deluxe", "Edge")],
                        overrides: Vec::new(),
                    },
                ],
            }],
        }
    }

    fn composed(snapshot: &str) -> ProfileDef {
        let mut def = worship_def();
        let p = &mut def.patches[0];
        p.rig_preset = "Fender".into();
        p.snapshot = snapshot.into();
        p.overrides.clear();
        def
    }

    #[test]
    fn a_preset_snapshot_resolves_to_its_modules_captures() {
        let flat = flatten(&composed("Clean"), &comp());
        let patch = &flat.patches[0];
        let pool = flat.presets.iter().find(|p| p.name == patch.preset).expect("synthesised");
        assert_eq!(pool.nam, "/caps/deluxe-clean.nam");
        assert_eq!(pool.cab, "/irs/1x12.wav");
        assert!(patch.preset2.is_empty(), "no second amp in this snapshot");
    }

    #[test]
    fn overrides_layer_module_then_preset_then_patch() {
        let mut def = composed("Clean");
        def.patches[0].overrides = vec![OverrideDef::set("Time", "VERB 1", "mix", 0.5)];
        let flat = flatten(&def, &comp());
        let mixes: Vec<f32> = flat.patches[0]
            .overrides
            .iter()
            .filter(|o| o.block == "VERB 1" && o.param == "mix")
            .map(|o| o.value)
            .collect();
        // Time module 0.2, then the preset snapshot 0.3, then the patch 0.5 —
        // applied in order, so the patch wins.
        assert_eq!(mixes, vec![0.2, 0.3, 0.5]);
        let built = crate::profiles::build_profile(&flat, &drive_presets());
        let verb = built.patches[0].chain.iter().find(|b| b.name == "VERB 1").unwrap();
        assert_eq!(verb.param_f32("mix"), Some(0.5));
    }

    #[test]
    fn another_snapshot_of_the_same_preset_loads_a_second_amp() {
        let flat = flatten(&composed("Edge"), &comp());
        let patch = &flat.patches[0];
        let r = flat.presets.iter().find(|p| p.name == patch.preset2).expect("Amp R loaded");
        assert_eq!(r.nam, "/caps/ac30.nam");
    }

    #[test]
    fn a_patchs_own_pick_replaces_the_presets_for_that_module() {
        let mut def = composed("Clean");
        def.patches[0].modules = vec![choice("Amp", "Deluxe", "Edge"), choice("Drive", "Klon", "On")];
        let flat = flatten(&def, &comp());
        let patch = &flat.patches[0];
        let pool = flat.presets.iter().find(|p| p.name == patch.preset).unwrap();
        assert_eq!(pool.nam, "/caps/deluxe-edge.nam", "the patch's amp pick won");
        let slot = patch.drives.iter().find(|d| d.block == "Drive 1").expect("drive pick");
        assert_eq!((slot.preset.as_str(), slot.option), ("JHS Morning Glory", 2));
        // The Time pick still comes from the preset snapshot.
        assert!(patch.overrides.iter().any(|o| o.block == "VERB 1"));
    }

    /// The live rig (the node model) plays a composed patch exactly as the
    /// builder does — captures, cabs, Amp R, and the patch's own pedals.
    #[test]
    fn the_live_rig_plays_a_composed_patch_like_the_builder() {
        let mut def = composed("Edge");
        def.patches[0].modules = vec![choice("Drive", "Klon", "On"), choice("Time", "Plate", "Short")];
        let flat = flatten(&def, &comp());
        let drives = drive_presets();
        let live = crate::nodes::to_nodes(&flat, &drives).to_profile(&flat, &drives);
        let built = crate::profiles::build_profile(&flat, &drives);
        let key = |c: &[signal_sampler::RigBlock], name: &str| {
            c.iter()
                .find(|b| b.name == name)
                .map(|b| (b.nam.clone(), b.ir.clone(), b.bypassed, b.param_f32("mix")))
        };
        for slot in ["Amp L", "Cab L", "Amp R", "Cab R", "Drive 1", "Drive 2", "VERB 1"] {
            assert_eq!(
                key(&live.patches[0].chain, slot),
                key(&built.patches[0].chain, slot),
                "{slot}"
            );
        }
        let d1 = live.patches[0].chain.iter().find(|b| b.name == "Drive 1").unwrap();
        assert!(d1.nam.contains("High Gain"), "the Drive snapshot's pedal: {}", d1.nam);
    }

    #[test]
    fn an_uncomposed_patch_is_untouched() {
        let def = worship_def();
        let flat = flatten(&def, &comp());
        assert_eq!(flat.patches[0].preset, def.patches[0].preset);
        assert_eq!(flat.patches[0].overrides.len(), def.patches[0].overrides.len());
        assert_eq!(flat.presets.len(), def.presets.len());
    }
}
