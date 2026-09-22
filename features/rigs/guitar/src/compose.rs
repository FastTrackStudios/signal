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
    /// The loudness calibration, dB — what [`level_presets`] measured this
    /// snapshot needs to sit at the target. A patch playing it takes this as
    /// its calibration (its own `trim_db` stays on top), so every preset and
    /// every snapshot of one arrives at the same loudness.
    #[facet(default)]
    pub level_db: f32,
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
                // The patch's own second amp wins over the snapshot's —
                // patch level is the last word, as with overrides.
                let r = pool("R", &snap.nam2, &snap.cab2);
                if patch.preset2.is_empty() {
                    patch.preset2 = r;
                }
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
            patch.level_db = snap.level_db;
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
                        level_db: 0.0,
                    },
                    PresetSnapshotDef {
                        name: "Edge".into(),
                        modules: vec![choice("Amp", "Deluxe", "Edge")],
                        overrides: Vec::new(),
                        level_db: 0.0,
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

/// What migrating a profile to compositions would produce.
#[derive(Clone, Debug)]
pub struct Migration {
    pub modules: Vec<ModulePresetDef>,
    pub presets: Vec<RigPresetDef>,
    /// The profile, its patches pointed at the new presets.
    pub profile: ProfileDef,
}

/// The family an amp capture belongs to: its name's first word, so "Fender
/// Clean" and "Fender DI" become snapshots of one Amp preset "Fender".
fn family(name: &str) -> (String, String) {
    let mut words = name.split_whitespace();
    let head = words.next().unwrap_or(name).to_string();
    let rest = words.collect::<Vec<_>>().join(" ");
    (head, if rest.is_empty() { "Default".to_string() } else { rest })
}

/// Propose compositions for a profile written the old way, without changing
/// a single sound: every pool capture becomes a snapshot of an Amp preset
/// (grouped by family), the profile's pedal board becomes a Drive preset,
/// and the patches on each amp family become snapshots of one preset
/// carrying their overrides. Patches already composed are left alone.
#[must_use]
pub fn propose(def: &ProfileDef) -> Migration {
    let mut modules: Vec<ModulePresetDef> = Vec::new();
    let mut amp_of = std::collections::HashMap::new();
    for p in &def.presets {
        let (fam, snap) = family(&p.name);
        let fam = format!("{} {fam}", def.name);
        let module = match modules.iter_mut().find(|m| m.module == "Amp" && m.name == fam) {
            Some(m) => m,
            None => {
                modules.push(ModulePresetDef { module: "Amp".into(), name: fam.clone(), snapshots: Vec::new() });
                modules.last_mut().expect("just pushed")
            }
        };
        module.snapshots.push(ModuleSnapshotDef {
            name: snap.clone(),
            nam: p.nam.clone(),
            cab: p.cab.clone(),
            ..ModuleSnapshotDef::default()
        });
        amp_of.insert(p.name.to_lowercase(), (fam, snap));
    }
    let board = format!("{} Board", def.name);
    if !def.drives.is_empty() {
        modules.push(ModulePresetDef {
            module: "Drive".into(),
            name: board.clone(),
            snapshots: vec![ModuleSnapshotDef {
                name: "Board".into(),
                drives: def.drives.clone(),
                ..ModuleSnapshotDef::default()
            }],
        });
    }

    let mut presets: Vec<RigPresetDef> = Vec::new();
    let mut profile = def.clone();
    for patch in &mut profile.patches {
        if !patch.rig_preset.is_empty() || !patch.modules.is_empty() {
            continue;
        }
        let Some((fam, snap)) = amp_of.get(&patch.preset.to_lowercase()).cloned() else {
            continue;
        };
        let mut picks = vec![ModuleChoiceDef { module: "Amp".into(), preset: fam.clone(), snapshot: snap }];
        if !def.drives.is_empty() {
            picks.push(ModuleChoiceDef { module: "Drive".into(), preset: board.clone(), snapshot: "Board".into() });
        }
        // Amp R stays a patch-level pick: no module snapshot holds it yet.
        if !patch.preset2.is_empty() {
            continue;
        }
        let preset_name = fam.trim_start_matches(&format!("{} ", def.name)).to_string();
        let preset_name = format!("{} {preset_name}", def.name);
        let entry = match presets.iter_mut().find(|p| p.name == preset_name) {
            Some(p) => p,
            None => {
                presets.push(RigPresetDef { name: preset_name.clone(), snapshots: Vec::new() });
                presets.last_mut().expect("just pushed")
            }
        };
        entry.snapshots.push(PresetSnapshotDef {
            name: patch.name.clone(),
            modules: std::mem::take(&mut picks),
            overrides: std::mem::take(&mut patch.overrides),
            level_db: 0.0,
        });
        patch.rig_preset = preset_name;
        patch.snapshot = patch.name.clone();
    }
    Migration { modules, presets, profile }
}

#[cfg(test)]
mod migration_tests {
    use super::*;
    use crate::profiles::{build_profile, drive_presets, worship_def};

    /// Migrating changes how the library is organised, never how a patch
    /// sounds: every block of every patch builds the same.
    #[test]
    fn migrating_a_profile_changes_no_sound() {
        let def = worship_def();
        let m = propose(&def);
        assert!(m.profile.patches.iter().all(|p| !p.rig_preset.is_empty()), "every patch composed");
        let comp = Compositions { modules: m.modules.clone(), presets: m.presets.clone() };
        let before = build_profile(&def, &drive_presets());
        let after = build_profile(&flatten(&m.profile, &comp), &drive_presets());
        for (a, b) in before.patches.iter().zip(&after.patches) {
            assert_eq!(a.chain.len(), b.chain.len(), "{}", a.name);
            for (x, y) in a.chain.iter().zip(&b.chain) {
                assert_eq!(
                    (&x.name, &x.nam, &x.ir, x.bypassed),
                    (&y.name, &y.nam, &y.ir, y.bypassed),
                    "{}",
                    a.name
                );
                for param in &x.params {
                    assert_eq!(y.param_f32(&param.name), x.param_f32(&param.name), "{} {} {}", a.name, x.name, param.name);
                }
            }
        }
    }

    #[test]
    fn captures_group_into_amp_presets_by_family() {
        let m = propose(&worship_def());
        let amps: Vec<_> = m.modules.iter().filter(|x| x.module == "Amp").collect();
        assert!(amps.iter().any(|a| a.snapshots.len() > 1), "some family has several captures");
    }
}

/// Migrate a profile on disk: propose, then (unless `dry_run`) merge the new
/// module presets and presets into the shared libraries — keeping any that
/// already exist by name — and save the profile pointed at them.
///
/// # Errors
///
/// When no profile has that name.
pub fn migrate_profile(name: &str, dry_run: bool) -> Result<Migration, String> {
    let lib = crate::library::RigLibrary::load_or_bootstrap();
    let def = lib
        .profiles
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(name))
        .cloned()
        .ok_or_else(|| format!("no profile named {name:?}"))?;
    let m = propose(&def);
    if !dry_run {
        let mut comp = crate::library::RigLibrary::load_compositions();
        for module in &m.modules {
            if comp.module(&module.module, &module.name).is_none() {
                comp.modules.push(module.clone());
            }
        }
        for preset in &m.presets {
            if comp.preset(&preset.name).is_none() {
                comp.presets.push(preset.clone());
            }
        }
        crate::library::RigLibrary::save_compositions(&comp);
        crate::library::RigLibrary::save_profile(&m.profile);
    }
    Ok(m)
}

/// One preset snapshot's measurement.
#[derive(Clone, Debug)]
pub struct SnapshotLevel {
    pub preset: String,
    pub snapshot: String,
    pub lufs: Option<f32>,
    pub level_db: f32,
}

/// Level every snapshot of every preset to the same loudness: build each
/// one's full chain on `base` (a profile, for its board), render the DI
/// reference through it, and set `level_db` to the distance from the
/// target. Measured with no calibration of its own, so a second pass
/// repeats the first. `threads` renders run at once (they are offline and
/// slow; the cache is shared).
#[must_use]
pub fn level_presets(
    comp: &mut Compositions,
    base: &ProfileDef,
    drives: &[crate::profiles::DrivePresetDef],
    sample_rate: u32,
    threads: usize,
) -> Vec<SnapshotLevel> {
    let jobs: Vec<(usize, usize)> = comp
        .presets
        .iter()
        .enumerate()
        .flat_map(|(p, preset)| (0..preset.snapshots.len()).map(move |s| (p, s)))
        .collect();
    let measure = |(p, s): (usize, usize)| -> Option<f32> {
        let preset = &comp.presets[p];
        let mut def = base.clone();
        let mut patch = def.patches.first()?.clone();
        patch.name = "level".into();
        patch.rig_preset = preset.name.clone();
        patch.snapshot = preset.snapshots[s].name.clone();
        patch.modules.clear();
        patch.overrides.clear();
        patch.drives.clear();
        patch.preset2.clear();
        patch.level_db = 0.0;
        patch.trim_db = 0.0;
        patch.boost_db = 0.0;
        def.patches = vec![patch];
        let mut calm = comp.clone();
        calm.presets[p].snapshots[s].level_db = 0.0;
        let flat = flatten(&def, &calm);
        let built = crate::profiles::build_profile(&flat, drives);
        let blocks: Vec<_> = built.patches.first()?.chain.iter().filter(|b| b.has_backend()).cloned().collect();
        signal_sampler::patch_level::level_of(&blocks, sample_rate)
            .filter(|l| l.is_finite() && *l > -70.0)
            .map(|l| l as f32)
    };
    let results = std::sync::Mutex::new(vec![None; jobs.len()]);
    let next = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..threads.max(1) {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(&job) = jobs.get(i) else { break };
                let lufs = measure(job);
                results.lock().unwrap_or_else(std::sync::PoisonError::into_inner)[i] = lufs;
            });
        }
    });
    let results = results.into_inner().unwrap_or_else(std::sync::PoisonError::into_inner);
    let target = signal_sampler::patch_level::TARGET_LUFS as f32;
    jobs.iter()
        .zip(results)
        .map(|(&(p, s), lufs)| {
            let preset = comp.presets[p].name.clone();
            let snap = &mut comp.presets[p].snapshots[s];
            if let Some(l) = lufs {
                // Wider than a patch's ±24: a library of amps spans clean
                // captures ~40 dB under a dimed amp-only one through an
                // un-normalised IR, and a clamp would leave them unlevelled.
                snap.level_db = (target - l).clamp(-40.0, 40.0);
            }
            SnapshotLevel {
                preset,
                snapshot: snap.name.clone(),
                lufs,
                level_db: snap.level_db,
            }
        })
        .collect()
}

/// Read an amp map: `old capture = Amp preset / snapshot` per line, `#`
/// comments. Keys compare case-insensitively.
#[must_use]
pub fn parse_amp_map(text: &str) -> Vec<(String, ModuleChoiceDef)> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let (old, new) = l.split_once('=')?;
            let (preset, snapshot) = new.rsplit_once(" / ")?;
            Some((
                old.trim().to_string(),
                ModuleChoiceDef {
                    module: "Amp".into(),
                    preset: preset.trim().to_string(),
                    snapshot: snapshot.trim().to_string(),
                },
            ))
        })
        .collect()
}

/// The shared pedalboard Drive preset.
pub const PEDALBOARD: &str = "Pedalboard";

/// Make a profile pure references: every uncomposed patch becomes a
/// snapshot of a preset named "<profile> <stack>" (the patch's name, its
/// overrides, its amp mapped through `amp_map`, the profile's board as a
/// Pedalboard snapshot), and the patch keeps only the reference. The pool,
/// the board, and per-patch levels leave the profile. Patches already
/// composed keep their pick. Returns the patches it could not map.
pub fn recompose(
    def: &mut ProfileDef,
    comp: &mut Compositions,
    amp_map: &[(String, ModuleChoiceDef)],
    drive_presets: &[crate::profiles::DrivePresetDef],
) -> Vec<String> {
    let mut unmapped = Vec::new();

    // The board, as a Pedalboard snapshot named by what each slot runs.
    let board_pick = (!def.drives.is_empty()).then(|| {
        let name = def
            .drives
            .iter()
            .map(|d| {
                drive_presets
                    .iter()
                    .find(|p| p.name.eq_ignore_ascii_case(&d.preset))
                    .and_then(|p| p.options.get(d.option))
                    .map_or_else(|| d.preset.clone(), |o| format!("{} {}", d.preset, o.name))
            })
            .collect::<Vec<_>>()
            .join(" + ");
        if !comp.modules.iter().any(|m| m.module == "Drive" && m.name == PEDALBOARD) {
            comp.modules.push(ModulePresetDef { module: "Drive".into(), name: PEDALBOARD.into(), snapshots: Vec::new() });
        }
        let board = comp
            .modules
            .iter_mut()
            .find(|m| m.module == "Drive" && m.name == PEDALBOARD)
            .expect("just ensured");
        if !board.snapshots.iter().any(|s| s.name == name) {
            board.snapshots.push(ModuleSnapshotDef { name: name.clone(), drives: def.drives.clone(), ..ModuleSnapshotDef::default() });
        }
        ModuleChoiceDef { module: "Drive".into(), preset: PEDALBOARD.into(), snapshot: name }
    });

    let stack_of = |patch: &str| {
        def.stacks
            .iter()
            .find(|s| s.patches.iter().any(|p| p.eq_ignore_ascii_case(patch)))
            .map_or_else(|| "Extra".to_string(), |s| s.name.clone())
    };
    let stacks: Vec<String> = def.patches.iter().map(|p| stack_of(&p.name)).collect();
    for (patch, stack) in def.patches.iter_mut().zip(stacks) {
        if patch.rig_preset.is_empty() {
            let Some((_, amp)) = amp_map.iter().find(|(old, _)| old.eq_ignore_ascii_case(&patch.preset)) else {
                unmapped.push(patch.name.clone());
                continue;
            };
            let mut modules = vec![amp.clone()];
            modules.extend(board_pick.clone());
            let preset_name = format!("{} {stack}", def.name);
            if comp.preset(&preset_name).is_none() {
                comp.presets.push(RigPresetDef { name: preset_name.clone(), snapshots: Vec::new() });
            }
            let preset = comp
                .presets
                .iter_mut()
                .find(|p| p.name.eq_ignore_ascii_case(&preset_name))
                .expect("just ensured");
            let snap = PresetSnapshotDef {
                name: patch.name.clone(),
                modules,
                overrides: std::mem::take(&mut patch.overrides),
                level_db: 0.0,
            };
            match preset.snapshots.iter_mut().find(|s| s.name.eq_ignore_ascii_case(&patch.name)) {
                Some(existing) => *existing = snap,
                None => preset.snapshots.push(snap),
            }
            patch.rig_preset = preset_name;
            patch.snapshot = patch.name.clone();
        }
        // What the preset now says, the patch no longer does.
        patch.preset.clear();
        patch.preset2.clear();
        patch.modules.clear();
        patch.drives.clear();
        patch.level_db = 0.0;
        patch.trim_db = 0.0;
    }
    if unmapped.is_empty() {
        def.presets.clear();
        def.drives.clear();
    }
    unmapped
}
