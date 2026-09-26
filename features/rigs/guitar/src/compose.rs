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

use crate::profiles::{
    DriveSlotDef, ModuleChoiceDef, OverrideDef, PatchDef, PresetDef, ProfileDef,
};

/// The block-preset library's file in the rig directory.
pub const BLOCKS_FILE: &str = "blocks.styx";
/// The module-preset library's file in the rig directory.
pub const MODULES_FILE: &str = "modules.styx";
/// The preset library's file.
pub const PRESETS_FILE: &str = "presets.styx";

/// The modules a preset composes, in signal order.
pub const MODULES: [&str; 7] =
    ["Dynamics", "Drive", "Amp", "Modulation", "Time", "Delay", "Reverb"];

/// One parameter a block preset sets.
#[derive(Clone, Debug, Default, Facet)]
pub struct ParamSetDef {
    pub param: String,
    pub value: f32,
}

/// How one macro moves one param, as a preset tunes it — where the param
/// lands at the knob's bottom and top, and the shape between.
///
/// On a block preset the entry is for the block the preset is on (`block`
/// empty); on a module snapshot it names the chain block (`DLY 1`). At rest
/// the param is always the patch's own value; turning up moves it toward
/// `max`, down toward `min`, along `curve`. A side left out does not move
/// that way. `off` keeps the macro off the param altogether (a slapback
/// that Space should never touch). With no entry the macro engine's own
/// relative response applies (see `crate::macros`).
#[derive(Clone, Debug, Default, PartialEq, Facet)]
pub struct MacroResponseDef {
    /// The chain block — a module snapshot's entries only.
    #[facet(default)]
    pub block: String,
    /// The macro: a bar knob (`delay`, `space`, `drive`) — its panel's knob
    /// on this param, or the bar knob itself — or one panel knob
    /// (`delay-fb1`).
    pub knob: String,
    pub param: String,
    /// Where the param lands at the knob's bottom, in the param's units.
    #[facet(default, skip_serializing_if = Option::is_none)]
    pub min: Option<f32>,
    /// Where it lands at the top.
    #[facet(default, skip_serializing_if = Option::is_none)]
    pub max: Option<f32>,
    /// `lin`, `log` (by ratio — times, frequencies), `exp` (slow, then
    /// fast) or `s`. Empty = `lin`.
    #[facet(default)]
    pub curve: String,
    #[facet(default)]
    pub off: bool,
    /// A Drive stage (a Drive module snapshot's entries): how far up the
    /// knob's upper half (0..1) the stage comes in — `min` is the drive it
    /// fades in from, `max` where it ends at the top.
    #[facet(default, skip_serializing_if = Option::is_none)]
    pub enter: Option<f32>,
}

impl MacroResponseDef {
    /// The bar knobs a list of entries tunes, once each, in order.
    #[must_use]
    pub fn knobs(list: &[Self]) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for r in list {
            let k = r.knob.split('-').next().unwrap_or(&r.knob).to_string();
            if !out.contains(&k) {
                out.push(k);
            }
        }
        out
    }
}

/// A preset for one kind of block — a compressor setting, an EQ curve, a
/// spring reverb — picked onto any chain block of that kind.
#[derive(Clone, Debug, Default, Facet)]
pub struct BlockPresetDef {
    /// The block type's storage key: `compressor`, `eq`, `reverb`, `gate`, …
    pub block_type: String,
    pub name: String,
    #[facet(default)]
    pub params: Vec<ParamSetDef>,
    /// The preset is "off": picking it bypasses the block.
    #[facet(default)]
    pub bypass: bool,
    /// A compressor preset that sits after the amp: the gain reduction (dB,
    /// averaged over playing) it is meant to apply. Each snapshot's own
    /// threshold is dialled to it on that snapshot's amp
    /// ([`dial_post_comp`]), because what reaches a post comp differs from
    /// amp to amp. 0 = a preset whose threshold is used as written — a pre
    /// comp, which only ever hears the guitar.
    #[facet(default)]
    pub target_gr_db: f32,
    /// How the macros move this preset's params ([`MacroResponseDef`]).
    #[facet(default)]
    pub macros: Vec<MacroResponseDef>,
}

/// Put a block preset on a named chain block.
#[derive(Clone, Debug, Default, PartialEq, Eq, Facet)]
pub struct BlockChoiceDef {
    /// The chain block, by name: `Post Comp`, `Pre Verb`, `Amp EQ`, …
    pub block: String,
    pub preset: String,
}

/// `blocks.styx`.
#[derive(Clone, Debug, Default, Facet)]
pub struct BlockLib {
    #[facet(default)]
    pub presets: Vec<BlockPresetDef>,
}

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
    /// Amp: the Output Level (dB) of Amp L — set by levelling the amp
    /// through its own cab (`signal rig level-modules`) so every amp
    /// snapshot comes out equally loud. It is the amp block's own output
    /// gain, written into the chain when it is built.
    #[facet(default)]
    pub level_db: f32,
    /// Amp: the same for Amp R (`nam2` through `cab2`).
    #[facet(default)]
    pub level2_db: f32,
    /// Drive: which pedal (and which of its captures) each slot runs.
    #[facet(default)]
    pub drives: Vec<DriveSlotDef>,
    /// Block presets on this module's blocks (applied before `overrides`).
    #[facet(default)]
    pub blocks: Vec<BlockChoiceDef>,
    /// Other modules this snapshot plays — the Time module's snapshots are
    /// a Delay module pick and a Reverb module pick. A patch or preset that
    /// picks this snapshot gets these picks too, unless it picks one of
    /// those modules itself ([`module_picks`]).
    #[facet(default)]
    pub modules: Vec<ModuleChoiceDef>,
    #[facet(default)]
    pub overrides: Vec<OverrideDef>,
    /// How the macros move this module's blocks while the snapshot plays,
    /// by block — over the block presets' own ([`MacroResponseDef`]). A
    /// Drive snapshot's entries (`knob: drive`, per slot) shape the drive
    /// journey: where each stage comes in and its drive range.
    #[facet(default)]
    pub macros: Vec<MacroResponseDef>,
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
    /// Block presets on any chain block (after the modules, before
    /// `overrides`).
    #[facet(default)]
    pub blocks: Vec<BlockChoiceDef>,
    #[facet(default)]
    pub overrides: Vec<OverrideDef>,
    /// The loudness calibration, dB — what [`level_presets`] measured this
    /// snapshot needs to sit at the target. A patch playing it takes this as
    /// its calibration (its own `trim_db` stays on top), so every preset and
    /// every snapshot of one arrives at the same loudness.
    #[facet(default)]
    pub level_db: f32,
    /// How far above the loudness target this snapshot is levelled, dB. An
    /// overdriven amp has no peaks left, so at equal measured loudness it
    /// sounds *smaller* than a clean one; a crunch or a lead sitting 1–3 dB
    /// over the cleans is what a real amp turned up does, and reads as level.
    /// Every levelling pass aims a snapshot, and each patch that plays it, at
    /// the target plus this ([`loudness_target`]).
    #[facet(default)]
    pub gain_bias_db: f32,
    /// Where the macro knobs sit when a patch plays this snapshot — part of
    /// its sound. A patch's own positions win knob by knob.
    #[facet(default)]
    pub macros: Vec<crate::profiles::MacroValueDef>,
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
    pub blocks: Vec<BlockPresetDef>,
}

impl Compositions {
    #[must_use]
    pub fn module(&self, module: &str, preset: &str) -> Option<&ModulePresetDef> {
        self.modules
            .iter()
            .find(|m| m.module.eq_ignore_ascii_case(module) && m.name.eq_ignore_ascii_case(preset))
    }

    #[must_use]
    pub fn block_preset(&self, name: &str) -> Option<&BlockPresetDef> {
        self.blocks
            .iter()
            .find(|b| b.name.eq_ignore_ascii_case(name))
    }

    /// A block preset as the overrides it stands for, on `block`. Unknown
    /// presets resolve to nothing (and say so).
    #[must_use]
    pub fn block_overrides(&self, choice: &BlockChoiceDef) -> Vec<OverrideDef> {
        let Some(p) = self.block_preset(&choice.preset) else {
            tracing::warn!(block = %choice.block, preset = %choice.preset, "compose: no such block preset");
            return Vec::new();
        };
        let mut out = vec![OverrideDef::bypass("", &choice.block, p.bypass)];
        if !p.bypass {
            out.extend(
                p.params
                    .iter()
                    .map(|x| OverrideDef::set("", &choice.block, &x.param, x.value)),
            );
        }
        out
    }

    #[must_use]
    pub fn preset(&self, name: &str) -> Option<&RigPresetDef> {
        self.presets
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
    }

    /// Module presets for one module, in library order.
    pub fn modules_of<'a>(
        &'a self,
        module: &'a str,
    ) -> impl Iterator<Item = &'a ModulePresetDef> + 'a {
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
    let mut picks: Vec<ModuleChoiceDef> = expand_picks(
        comp,
        comp.preset(&patch.rig_preset)
            .and_then(|p| snapshot(&p.snapshots, &patch.snapshot, |s| &s.name))
            .map(|s| s.modules.clone())
            .unwrap_or_default(),
    );
    // The patch's own picks, each with the modules it plays (a Time pick
    // brings its Delay and Reverb), over the preset's.
    for own in &expand_picks(comp, patch.modules.clone()) {
        match picks
            .iter_mut()
            .find(|p| p.module.eq_ignore_ascii_case(&own.module))
        {
            Some(p) => *p = own.clone(),
            None => picks.push(own.clone()),
        }
    }
    picks
}

/// The block presets a patch ends up with, one per block: its module
/// snapshots' (in signal order), then its preset snapshot's, then its own —
/// the last word per block, as `flatten` applies them.
#[must_use]
pub fn block_picks(comp: &Compositions, patch: &PatchDef) -> Vec<BlockChoiceDef> {
    let mut out: Vec<BlockChoiceDef> = Vec::new();
    let mut put = |c: &BlockChoiceDef| match out
        .iter_mut()
        .find(|x| x.block.eq_ignore_ascii_case(&c.block))
    {
        Some(x) => *x = c.clone(),
        None => out.push(c.clone()),
    };
    let picks = module_picks(comp, patch);
    for m in MODULES
        .iter()
        .filter_map(|m| picks.iter().find(|p| p.module.eq_ignore_ascii_case(m)))
        .chain(picks.iter().filter(|p| !MODULES.iter().any(|m| p.module.eq_ignore_ascii_case(m))))
    {
        if let Some(snap) = comp
            .module(&m.module, &m.preset)
            .and_then(|mp| snapshot(&mp.snapshots, &m.snapshot, |s| &s.name))
        {
            snap.blocks.iter().for_each(&mut put);
        }
    }
    if let Some(snap) = comp
        .preset(&patch.rig_preset)
        .and_then(|p| snapshot(&p.snapshots, &patch.snapshot, |s| &s.name))
    {
        snap.blocks.iter().for_each(&mut put);
    }
    patch.blocks.iter().for_each(&mut put);
    out
}

/// The chain blocks a module snapshot sets — through its block presets and
/// overrides, and those of the modules it references (a Time snapshot's
/// Delay and Reverb) — by block name.
#[must_use]
pub fn blocks_set_by(comp: &Compositions, module: &str, preset: &str, snapshot_name: &str) -> Vec<String> {
    fn walk(comp: &Compositions, m: &str, p: &str, s: &str, depth: u8, out: &mut Vec<String>) {
        if depth > 4 {
            return;
        }
        let Some(snap) = comp
            .module(m, p)
            .and_then(|mp| snapshot(&mp.snapshots, s, |x| &x.name))
        else {
            return;
        };
        let mut add = |b: &str| {
            if !b.is_empty() && !out.iter().any(|x| x.eq_ignore_ascii_case(b)) {
                out.push(b.to_string());
            }
        };
        snap.blocks.iter().for_each(|c| add(&c.block));
        snap.overrides.iter().for_each(|o| add(&o.block));
        for sub in &snap.modules {
            walk(comp, &sub.module, &sub.preset, &sub.snapshot, depth + 1, out);
        }
    }
    let mut out = Vec::new();
    walk(comp, module, preset, snapshot_name, 0, &mut out);
    out
}

/// A layer of module picks with the picks their snapshots play added (a
/// Time snapshot's Delay and Reverb), except for modules the layer picks
/// itself — an explicit pick at the same level wins.
#[must_use]
pub fn expand_picks(comp: &Compositions, layer: Vec<ModuleChoiceDef>) -> Vec<ModuleChoiceDef> {
    let mut out = layer.clone();
    for p in &layer {
        let Some(snap) = comp
            .module(&p.module, &p.preset)
            .and_then(|m| snapshot(&m.snapshots, &p.snapshot, |s| &s.name))
        else {
            continue;
        };
        for sub in &snap.modules {
            if !out.iter().any(|o| o.module.eq_ignore_ascii_case(&sub.module)) {
                out.push(sub.clone());
            }
        }
    }
    out
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
        patch.overrides.iter_mut().for_each(OverrideDef::pin_parallel_mix);
        if patch.rig_preset.is_empty() && patch.modules.is_empty() && patch.blocks.is_empty() {
            continue;
        }
        let picks = module_picks(comp, patch);
        let mut overrides: Vec<OverrideDef> = Vec::new();

        // Module snapshots first, in signal order, then anything unknown.
        let ordered = MODULES
            .iter()
            .filter_map(|m| picks.iter().find(|p| p.module.eq_ignore_ascii_case(m)))
            .chain(
                picks
                    .iter()
                    .filter(|p| !MODULES.iter().any(|m| p.module.eq_ignore_ascii_case(m))),
            );
        for pick in ordered {
            let Some(module) = comp.module(&pick.module, &pick.preset) else {
                tracing::warn!(patch = %patch.name, module = %pick.module, preset = %pick.preset, "compose: no such module preset");
                continue;
            };
            let Some(snap) = snapshot(&module.snapshots, &pick.snapshot, |s| &s.name) else {
                continue;
            };
            if pick.module.eq_ignore_ascii_case("Amp") {
                let mut pool = |slot: &str, nam: &str, cab: &str, level_db: f32| -> String {
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
                            level_db,
                        });
                    }
                    name
                };
                let l = pool("L", &snap.nam, &snap.cab, snap.level_db);
                if !l.is_empty() {
                    patch.preset = l;
                }
                // The patch's own second amp wins over the snapshot's —
                // patch level is the last word, as with overrides.
                let r = pool("R", &snap.nam2, &snap.cab2, snap.level2_db);
                if patch.preset2.is_empty() {
                    patch.preset2 = r;
                }
            }
            for choice in &snap.blocks {
                overrides.extend(comp.block_overrides(choice));
            }
            for d in &snap.drives {
                match patch
                    .drives
                    .iter_mut()
                    .find(|x| x.block.eq_ignore_ascii_case(&d.block))
                {
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
            for choice in &snap.blocks {
                overrides.extend(comp.block_overrides(choice));
            }
            overrides.extend(snap.overrides.iter().cloned());
            // The snapshot's level, plus the patch's own offset on top: a
            // snapshot is levelled on its own, and a patch that adds overrides
            // (an EQ move, a trim) is corrected by its own `level_db` — which
            // an overwrite here threw away, so a patch-level fix could never
            // take.
            patch.level_db += snap.level_db;
        }
        // The patch's own block presets, over everything above.
        for choice in &patch.blocks {
            overrides.extend(comp.block_overrides(choice));
        }
        overrides.append(&mut patch.overrides);
        overrides.iter_mut().for_each(OverrideDef::pin_parallel_mix);
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
        match out
            .iter_mut()
            .find(|x| x.block.eq_ignore_ascii_case(&d.block))
        {
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
            blocks: Vec::new(),
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
                        blocks: Vec::new(),
                        name: "Clean".into(),
                        modules: vec![
                            choice("Amp", "Deluxe", "Clean"),
                            choice("Time", "Plate", "Short"),
                        ],
                        overrides: vec![OverrideDef::set("Time", "VERB 1", "mix", 0.3)],
                        level_db: 0.0,
                        gain_bias_db: 0.0,
                        macros: Vec::new(),
                    },
                    PresetSnapshotDef {
                        blocks: Vec::new(),
                        name: "Edge".into(),
                        modules: vec![choice("Amp", "Deluxe", "Edge")],
                        overrides: Vec::new(),
                        level_db: 0.0,
                        gain_bias_db: 2.0,
                        macros: Vec::new(),
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

    /// A Time snapshot plays the Delay and Reverb picks it references; a
    /// patch's own Delay pick still wins over the one its Time brings.
    #[test]
    fn a_time_pick_brings_its_delay_and_reverb() {
        let snap = |name: &str, modules: Vec<crate::profiles::ModuleChoiceDef>| ModuleSnapshotDef {
            name: name.into(),
            modules,
            ..ModuleSnapshotDef::default()
        };
        let c = Compositions {
            modules: vec![ModulePresetDef {
                module: "Time".into(),
                name: "Rhythmic".into(),
                snapshots: vec![snap(
                    "Dotted",
                    vec![choice("Delay", "Rhythmic", "Dotted Eighth"), choice("Reverb", "Hall", "Hall")],
                )],
            }],
            ..Compositions::default()
        };
        let mut patch = crate::profiles::PatchDef {
            modules: vec![choice("Time", "Rhythmic", "Dotted")],
            ..snapshot_patch(&worship_def(), "", "", "p").expect("a patch")
        };
        let picks = module_picks(&c, &patch);
        let of = |m: &str| picks.iter().find(|p| p.module == m).map(|p| p.snapshot.clone());
        assert_eq!(of("Delay").as_deref(), Some("Dotted Eighth"));
        assert_eq!(of("Reverb").as_deref(), Some("Hall"));
        patch.modules.push(choice("Delay", "Slapback", "Slap"));
        let picks = module_picks(&c, &patch);
        let of = |m: &str| picks.iter().find(|p| p.module == m).map(|p| p.snapshot.clone());
        assert_eq!(of("Delay").as_deref(), Some("Slap"), "the patch's own Delay wins");
        assert_eq!(of("Reverb").as_deref(), Some("Hall"));
    }

    /// A snapshot's gain bias lifts its loudness target; one without, or a
    /// patch with no snapshot, levels to the rig's target.
    #[test]
    fn loudness_target_adds_the_snapshot_gain_bias() {
        let c = comp();
        let base = signal_sampler::patch_level::TARGET_LUFS as f32;
        assert!((loudness_target(&c, "Fender", "Edge") - (base + 2.0)).abs() < 1e-6);
        assert!((loudness_target(&c, "Fender", "Clean") - base).abs() < 1e-6);
        assert!((loudness_target(&c, "", "") - base).abs() < 1e-6);
    }

    #[test]
    fn a_preset_snapshot_resolves_to_its_modules_captures() {
        let flat = flatten(&composed("Clean"), &comp());
        let patch = &flat.patches[0];
        let pool = flat
            .presets
            .iter()
            .find(|p| p.name == patch.preset)
            .expect("synthesised");
        assert_eq!(pool.nam, "/caps/deluxe-clean.nam");
        assert_eq!(pool.cab, "/irs/1x12.wav");
        assert!(patch.preset2.is_empty(), "no second amp in this snapshot");
    }

    #[test]
    fn overrides_layer_module_then_preset_then_patch() {
        let mut def = composed("Clean");
        def.patches[0].overrides = vec![OverrideDef::set("Time", "VERB 1", "mix", 0.5)];
        let flat = flatten(&def, &comp());
        let levels: Vec<f32> = flat.patches[0]
            .overrides
            .iter()
            .filter(|o| o.block == "VERB 1" && o.param == "level")
            .map(|o| o.value)
            .collect();
        // Time module 0.2, then the preset snapshot 0.3, then the patch 0.5 —
        // applied in order, so the patch wins. Each is a `mix` written before
        // the reverbs ran fully wet: it arrives as the `level` that plays
        // the same.
        let db = crate::profiles::mix_to_level_db;
        assert_eq!(levels, vec![db(0.2), db(0.3), db(0.5)]);
        let built = crate::profiles::build_profile(&flat, &drive_presets());
        let verb = built.patches[0]
            .chain
            .iter()
            .find(|b| b.name == "VERB 1")
            .unwrap();
        assert_eq!(verb.param_f32("level"), Some(db(0.5)));
        assert_eq!(verb.param_f32("mix"), Some(1.0), "fully wet");
    }

    #[test]
    fn another_snapshot_of_the_same_preset_loads_a_second_amp() {
        let flat = flatten(&composed("Edge"), &comp());
        let patch = &flat.patches[0];
        let r = flat
            .presets
            .iter()
            .find(|p| p.name == patch.preset2)
            .expect("Amp R loaded");
        assert_eq!(r.nam, "/caps/ac30.nam");
    }

    #[test]
    fn a_patchs_own_pick_replaces_the_presets_for_that_module() {
        let mut def = composed("Clean");
        def.patches[0].modules = vec![
            choice("Amp", "Deluxe", "Edge"),
            choice("Drive", "Klon", "On"),
        ];
        let flat = flatten(&def, &comp());
        let patch = &flat.patches[0];
        let pool = flat
            .presets
            .iter()
            .find(|p| p.name == patch.preset)
            .unwrap();
        assert_eq!(
            pool.nam, "/caps/deluxe-edge.nam",
            "the patch's amp pick won"
        );
        let slot = patch
            .drives
            .iter()
            .find(|d| d.block == "Drive 1")
            .expect("drive pick");
        assert_eq!(
            (slot.preset.as_str(), slot.option),
            ("JHS Morning Glory", 2)
        );
        // The Time pick still comes from the preset snapshot.
        assert!(patch.overrides.iter().any(|o| o.block == "VERB 1"));
    }

    /// The live rig (the node model) plays a composed patch exactly as the
    /// builder does — captures, cabs, Amp R, and the patch's own pedals.
    #[test]
    fn the_live_rig_plays_a_composed_patch_like_the_builder() {
        let mut def = composed("Edge");
        def.patches[0].modules = vec![
            choice("Drive", "Klon", "On"),
            choice("Time", "Plate", "Short"),
        ];
        let flat = flatten(&def, &comp());
        let drives = drive_presets();
        let live = crate::nodes::to_nodes(&flat, &drives).to_profile(&flat, &drives);
        let built = crate::profiles::build_profile(&flat, &drives);
        let key = |c: &[signal_sampler::RigBlock], name: &str| {
            c.iter()
                .find(|b| b.name == name)
                .map(|b| (b.nam.clone(), b.ir.clone(), b.bypassed, b.param_f32("mix")))
        };
        for slot in [
            "Amp L", "Cab L", "Amp R", "Cab R", "Drive 1", "Drive 2", "VERB 1",
        ] {
            assert_eq!(
                key(&live.patches[0].chain, slot),
                key(&built.patches[0].chain, slot),
                "{slot}"
            );
        }
        let d1 = live.patches[0]
            .chain
            .iter()
            .find(|b| b.name == "Drive 1")
            .unwrap();
        assert!(
            d1.nam.contains("High Gain"),
            "the Drive snapshot's pedal: {}",
            d1.nam
        );
    }

    /// A block preset on a module snapshot engages its block and sets its
    /// parameters; the patch's own override still has the last word.
    #[test]
    fn a_block_preset_sets_its_block_and_overrides_still_win() {
        let mut c = comp();
        c.blocks.push(BlockPresetDef {
            block_type: "compressor".into(),
            name: "Studio Glue".into(),
            params: vec![
                ParamSetDef {
                    param: "ratio".into(),
                    value: 3.0,
                },
                ParamSetDef {
                    param: "attack".into(),
                    value: 20.0,
                },
            ],
            bypass: false,
            target_gr_db: 0.0,
            macros: Vec::new(),
        });
        c.modules[0].snapshots[0].blocks.push(BlockChoiceDef {
            block: crate::profiles::POST_COMP.into(),
            preset: "Studio Glue".into(),
        });
        let mut def = composed("Clean");
        def.patches[0].overrides = vec![OverrideDef::set(
            "Amp",
            crate::profiles::POST_COMP,
            "ratio",
            4.0,
        )];
        let flat = flatten(&def, &c);
        let built = crate::profiles::build_profile(&flat, &drive_presets());
        let comp_block = built.patches[0]
            .chain
            .iter()
            .find(|b| b.name == crate::profiles::POST_COMP)
            .expect("Post Comp");
        assert!(!comp_block.bypassed, "the preset engages it");
        assert_eq!(comp_block.param_f32("attack"), Some(20.0));
        assert_eq!(
            comp_block.param_f32("ratio"),
            Some(4.0),
            "the patch's override wins"
        );
    }

    #[test]
    fn an_uncomposed_patch_is_untouched() {
        let def = worship_def();
        let flat = flatten(&def, &comp());
        assert_eq!(flat.patches[0].preset, def.patches[0].preset);
        assert_eq!(
            flat.patches[0].overrides.len(),
            def.patches[0].overrides.len()
        );
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
    (
        head,
        if rest.is_empty() {
            "Default".to_string()
        } else {
            rest
        },
    )
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
        let module = match modules
            .iter_mut()
            .find(|m| m.module == "Amp" && m.name == fam)
        {
            Some(m) => m,
            None => {
                modules.push(ModulePresetDef {
                    module: "Amp".into(),
                    name: fam.clone(),
                    snapshots: Vec::new(),
                });
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
        let mut picks = vec![ModuleChoiceDef {
            module: "Amp".into(),
            preset: fam.clone(),
            snapshot: snap,
        }];
        if !def.drives.is_empty() {
            picks.push(ModuleChoiceDef {
                module: "Drive".into(),
                preset: board.clone(),
                snapshot: "Board".into(),
            });
        }
        // Amp R stays a patch-level pick: no module snapshot holds it yet.
        if !patch.preset2.is_empty() {
            continue;
        }
        let preset_name = fam
            .trim_start_matches(&format!("{} ", def.name))
            .to_string();
        let preset_name = format!("{} {preset_name}", def.name);
        let entry = match presets.iter_mut().find(|p| p.name == preset_name) {
            Some(p) => p,
            None => {
                presets.push(RigPresetDef {
                    name: preset_name.clone(),
                    snapshots: Vec::new(),
                });
                presets.last_mut().expect("just pushed")
            }
        };
        entry.snapshots.push(PresetSnapshotDef {
            blocks: Vec::new(),
            name: patch.name.clone(),
            modules: std::mem::take(&mut picks),
            overrides: std::mem::take(&mut patch.overrides),
            level_db: 0.0,
            gain_bias_db: 0.0,
            macros: Vec::new(),
        });
        patch.rig_preset = preset_name;
        patch.snapshot = patch.name.clone();
    }
    Migration {
        modules,
        presets,
        profile,
    }
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
        assert!(
            m.profile.patches.iter().all(|p| !p.rig_preset.is_empty()),
            "every patch composed"
        );
        let comp = Compositions {
            modules: m.modules.clone(),
            presets: m.presets.clone(),
            blocks: Vec::new(),
        };
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
                    assert_eq!(
                        y.param_f32(&param.name),
                        x.param_f32(&param.name),
                        "{} {} {}",
                        a.name,
                        x.name,
                        param.name
                    );
                }
            }
        }
    }

    #[test]
    fn captures_group_into_amp_presets_by_family() {
        let m = propose(&worship_def());
        let amps: Vec<_> = m.modules.iter().filter(|x| x.module == "Amp").collect();
        assert!(
            amps.iter().any(|a| a.snapshots.len() > 1),
            "some family has several captures"
        );
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
    // A snapshot's loudness *with* `level_db` applied — built exactly as the
    // live rig builds a patch (`nodes::profile_from_library`'s path, with
    // this composition in place of the saved one) and measured on the rig.
    let measure_at = |(p, s): (usize, usize), level_db: f32| -> Option<f32> {
        let preset = &comp.presets[p];
        let mut def = base.clone();
        def.patches = vec![snapshot_patch(base, &preset.name, &preset.snapshots[s].name, "level")?];
        let mut calm = comp.clone();
        calm.presets[p].snapshots[s].level_db = level_db;
        let flat = flatten(&def, &calm);
        let mut built = crate::nodes::to_nodes_with_store(&flat, drives).to_profile(&flat, drives);
        // With the snapshot's macro knob positions — part of its sound.
        let patch = built.patches.first_mut()?;
        crate::macros::apply_positions_for_level(flat.patches.first()?, &calm, patch);
        crate::measure::patch_lufs(patch, sample_rate).filter(|l| *l > -70.0)
    };
    // Measure raw, correct, and re-measure with the correction applied until
    // it lands: the chain ends in the rig's limiter (the live chain does), so
    // a hot snapshot reads quieter than it is and one subtraction undershoots.
    // After the first step the trimmed signal sits under the limiter and the
    // next one is exact; cached, so a re-run costs nothing.
    let measure = |&job: &(usize, usize)| -> Option<(f32, f32)> {
        let target = signal_sampler::patch_level::TARGET_LUFS as f32
            + comp.presets[job.0].snapshots[job.1].gain_bias_db;
        let raw = measure_at(job, 0.0)?;
        let mut level = (target - raw).clamp(-40.0, 40.0);
        for _ in 0..3 {
            let at = measure_at(job, level)?;
            if (target - at).abs() <= 0.2 {
                break;
            }
            level = (level + (target - at)).clamp(-40.0, 40.0);
        }
        Some((raw, level))
    };
    let results = crate::levelling::par_map(&jobs, threads, measure);
    jobs.iter()
        .zip(results)
        .map(|(&(p, s), measured)| {
            let lufs = measured.map(|(raw, _)| raw);
            let preset = comp.presets[p].name.clone();
            let snap = &mut comp.presets[p].snapshots[s];
            // Wider than a patch's ±24: a library of amps spans clean
            // captures ~40 dB under a dimed amp-only one through an
            // un-normalised IR, and a clamp would leave them unlevelled.
            if let Some((_, level)) = measured {
                snap.level_db = level;
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

/// One snapshot moved by [`regroup_by_gear`].
#[derive(Clone, Debug)]
pub struct Regrouped {
    pub from_preset: String,
    pub from_snapshot: String,
    pub to_preset: String,
    pub to_snapshot: String,
    /// An identical snapshot was already there, and is now shared.
    pub reused: bool,
}

/// Name every preset for the gear it plays, never for a profile.
///
/// A preset is *gear* when every snapshot picks the Amp module preset of its
/// own name (`Fender Deluxe Reverb`'s snapshots all pick the Fender Deluxe
/// Reverb amp). Anything else — the `Worship Clean`s and `Metal Rhythm`s
/// that recomposing a profile left behind — is dissolved: each snapshot
/// moves into the gear preset of the amp it picks, reusing an identical one
/// already there (same modules, blocks and overrides) or joining under its
/// own name (numbered if that is taken). Every patch in `profiles` that
/// pointed at a moved snapshot is repointed, so every patch plays exactly
/// what it did. A snapshot with no Amp pick stays where it is.
pub fn regroup_by_gear(comp: &mut Compositions, profiles: &mut [ProfileDef]) -> Vec<Regrouped> {
    fn amp_of(snapshot: &PresetSnapshotDef) -> Option<String> {
        snapshot
            .modules
            .iter()
            .find(|m| m.module.eq_ignore_ascii_case("Amp"))
            .map(|m| m.preset.clone())
    }
    // What a snapshot *is*, for spotting an identical one.
    fn content(snapshot: &PresetSnapshotDef) -> String {
        format!("{:?}|{:?}|{:?}", snapshot.modules, snapshot.blocks, snapshot.overrides)
    }
    let is_gear = |p: &RigPresetDef| {
        !p.snapshots.is_empty()
            && p.snapshots
                .iter()
                .all(|s| amp_of(s).is_some_and(|a| a.eq_ignore_ascii_case(&p.name)))
    };

    let mut moved = Vec::new();
    let composites: Vec<RigPresetDef> = comp.presets.iter().filter(|p| !is_gear(p)).cloned().collect();
    for composite in composites {
        let mut kept = Vec::new();
        for snap in composite.snapshots {
            let Some(amp) = amp_of(&snap) else {
                kept.push(snap);
                continue;
            };
            let gear = match comp.presets.iter().position(|p| p.name.eq_ignore_ascii_case(&amp)) {
                Some(i) => i,
                None => {
                    comp.presets.push(RigPresetDef {
                        name: amp.clone(),
                        snapshots: Vec::new(),
                    });
                    comp.presets.len() - 1
                }
            };
            let key = content(&snap);
            let target = &mut comp.presets[gear];
            let (to_snapshot, reused) = if let Some(same) = target.snapshots.iter().find(|s| content(s) == key) {
                (same.name.clone(), true)
            } else {
                // Its own name if free; else named by what sets it apart —
                // the drive pedal it adds, or failing that its delay — and
                // only then numbered.
                let taken = |target: &RigPresetDef, name: &str| {
                    target.snapshots.iter().any(|s| s.name.eq_ignore_ascii_case(name))
                };
                let pick = |module: &str| {
                    snap.modules
                        .iter()
                        .find(|m| m.module.eq_ignore_ascii_case(module))
                        .filter(|m| !m.preset.eq_ignore_ascii_case("Off"))
                };
                let mut name = snap.name.clone();
                if taken(target, &name) {
                    if let Some(drive) = pick("Drive") {
                        name = format!("{} + {}", snap.name, drive.preset);
                    } else if let Some(time) = pick("Delay").or_else(|| pick("Time")) {
                        name = format!("{} · {}", snap.name, time.snapshot);
                    }
                }
                let base = name.clone();
                let mut n = 2;
                while taken(target, &name) {
                    name = format!("{base} {n}");
                    n += 1;
                }
                let mut joined = snap.clone();
                joined.name.clone_from(&name);
                target.snapshots.push(joined);
                (name, false)
            };
            moved.push(Regrouped {
                from_preset: composite.name.clone(),
                from_snapshot: snap.name.clone(),
                to_preset: target.name.clone(),
                to_snapshot,
                reused,
            });
        }
        let at = comp
            .presets
            .iter()
            .position(|p| p.name.eq_ignore_ascii_case(&composite.name));
        match (at, kept.is_empty()) {
            (Some(i), true) => {
                comp.presets.remove(i);
            }
            (Some(i), false) => comp.presets[i].snapshots = kept,
            (None, _) => {}
        }
    }

    for profile in profiles.iter_mut() {
        for patch in &mut profile.patches {
            if let Some(m) = moved.iter().find(|m| {
                m.from_preset.eq_ignore_ascii_case(&patch.rig_preset)
                    && m.from_snapshot.eq_ignore_ascii_case(&patch.snapshot)
            }) {
                patch.rig_preset.clone_from(&m.to_preset);
                patch.snapshot.clone_from(&m.to_snapshot);
            }
        }
    }
    moved
}

/// One snapshot's dialled Post Comp.
#[derive(Clone, Debug)]
pub struct PostCompDial {
    pub preset: String,
    pub snapshot: String,
    /// The block preset on its Post Comp.
    pub comp: String,
    pub target_gr_db: f32,
    /// The threshold found, and the gain reduction it measured; `None` when
    /// the chain did not render.
    pub dialled: Option<(f32, f32)>,
}

/// The block every post-comp dial works on.
const POST_COMP: &str = crate::profiles::POST_COMP;

/// Dial each snapshot's Post Comp threshold to its preset's
/// [`target_gr_db`](BlockPresetDef::target_gr_db), on that snapshot's own
/// chain: built as the rig builds it (drives, Pre Comp, amps), cut after the
/// Post Comp, and rendered with it engaged and bypassed at no makeup — the
/// loudness difference is the gain reduction. The threshold is written as
/// the snapshot's own override, so the block preset still carries the
/// character (ratio, attack, release, knee, style) for every snapshot that
/// uses it. Snapshots whose Post Comp preset has no target are left alone.
///
/// Changes each snapshot's level; run [`level_presets`] after.
#[must_use]
pub fn dial_post_comp(
    comp: &mut Compositions,
    base: &ProfileDef,
    drives: &[crate::profiles::DrivePresetDef],
    sample_rate: u32,
    threads: usize,
    only: Option<&str>,
) -> Vec<PostCompDial> {
    let jobs: Vec<(usize, usize, String, f32)> = comp
        .presets
        .iter()
        .enumerate()
        // `only`: one preset (by name), when just its snapshots changed.
        .filter(|(_, preset)| only.is_none_or(|n| preset.name.eq_ignore_ascii_case(n)))
        .flat_map(|(p, preset)| {
            let comp = &*comp;
            preset.snapshots.iter().enumerate().filter_map(move |(s, snap)| {
                let choice = snap
                    .blocks
                    .iter()
                    .rev()
                    .find(|b| b.block.eq_ignore_ascii_case(POST_COMP))?;
                let target = comp.block_preset(&choice.preset)?.target_gr_db;
                (target > 0.0).then(|| (p, s, choice.preset.clone(), target))
            })
        })
        .collect();

    // The snapshot's chain up to its Post Comp, with the comp at `threshold`
    // and engaged or not.
    let render = |p: usize, s: usize, threshold: f32, engaged: bool| -> Option<f32> {
        let preset = &comp.presets[p];
        let mut def = base.clone();
        def.patches = vec![snapshot_patch(base, &preset.name, &preset.snapshots[s].name, "dial")?];
        let mut trial = comp.clone();
        let snap = &mut trial.presets[p].snapshots[s];
        snap.overrides.retain(|o| {
            !(o.block.eq_ignore_ascii_case(POST_COMP) && o.param.eq_ignore_ascii_case("threshold"))
        });
        snap.overrides.push(OverrideDef::set("", POST_COMP, "threshold", threshold));
        snap.overrides.push(OverrideDef::set("", POST_COMP, "makeup", 0.0));
        let flat = flatten(&def, &trial);
        let mut patch = crate::nodes::to_nodes_with_store(&flat, drives)
            .to_profile(&flat, drives)
            .patches
            .into_iter()
            .next()?;
        let mut after = false;
        for b in &mut patch.chain {
            if after {
                b.bypassed = true;
            } else if b.name.eq_ignore_ascii_case(POST_COMP) {
                after = true;
                b.bypassed = !engaged;
            }
        }
        crate::measure::patch_lufs(&patch, sample_rate).filter(|l| *l > -70.0)
    };

    let dialled = crate::levelling::par_map(&jobs, threads, |&(p, s, _, target)| {
        let dry = render(p, s, 0.0, false)?;
        let gr = |t: f32| render(p, s, t, true).map(|wet| dry - wet);
        // Gain reduction falls as the threshold rises: bisect on it.
        let (mut lo, mut hi) = (-50.0f32, 0.0f32);
        for _ in 0..8 {
            let mid = 0.5 * (lo + hi);
            if gr(mid)? > target {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let threshold = (lo + hi).round() / 2.0;
        Some((threshold, gr(threshold)?))
    });

    jobs.iter()
        .zip(dialled)
        .map(|(&(p, s, ref name, target), dialled)| {
            let snap = &mut comp.presets[p].snapshots[s];
            if let Some((threshold, _)) = dialled {
                snap.overrides.retain(|o| {
                    !(o.block.eq_ignore_ascii_case(POST_COMP)
                        && o.param.eq_ignore_ascii_case("threshold"))
                });
                snap.overrides.push(OverrideDef::set("", POST_COMP, "threshold", threshold));
            }
            PostCompDial {
                preset: comp.presets[p].name.clone(),
                snapshot: comp.presets[p].snapshots[s].name.clone(),
                comp: name.clone(),
                target_gr_db: target,
                dialled,
            }
        })
        .collect()
}

/// The loudness (LUFS) a patch playing `preset` / `snapshot` is levelled to:
/// the rig's target plus that snapshot's [`gain_bias_db`](PresetSnapshotDef::gain_bias_db).
/// A patch with no preset snapshot (the legacy pool shape) gets the target.
#[must_use]
pub fn loudness_target(comp: &Compositions, preset: &str, snapshot: &str) -> f32 {
    let bias = comp
        .preset(preset)
        .and_then(|p| {
            p.snapshots
                .iter()
                .find(|s| s.name.eq_ignore_ascii_case(snapshot))
        })
        .map_or(0.0, |s| s.gain_bias_db);
    signal_sampler::patch_level::TARGET_LUFS as f32 + bias
}

/// A patch that plays one preset snapshot and nothing else of its own: no
/// module picks, overrides, drives, second amp, level, trim or boost — the
/// snapshot as it is. Built on the profile's first patch for the chain
/// around it. It is what snapshot levelling measures, and what the preset
/// tab plays when it auditions a snapshot, so an audition lands exactly at
/// the level the snapshot was levelled to.
#[must_use]
pub fn snapshot_patch(base: &ProfileDef, preset: &str, snapshot: &str, name: &str) -> Option<crate::profiles::PatchDef> {
    let mut patch = base.patches.first()?.clone();
    patch.name = name.into();
    patch.rig_preset = preset.into();
    patch.snapshot = snapshot.into();
    patch.modules.clear();
    patch.overrides.clear();
    patch.drives.clear();
    patch.preset2.clear();
    patch.level_db = 0.0;
    patch.trim_db = 0.0;
    patch.boost_db = 0.0;
    Some(patch)
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
        if !comp
            .modules
            .iter()
            .any(|m| m.module == "Drive" && m.name == PEDALBOARD)
        {
            comp.modules.push(ModulePresetDef {
                module: "Drive".into(),
                name: PEDALBOARD.into(),
                snapshots: Vec::new(),
            });
        }
        let board = comp
            .modules
            .iter_mut()
            .find(|m| m.module == "Drive" && m.name == PEDALBOARD)
            .expect("just ensured");
        if !board.snapshots.iter().any(|s| s.name == name) {
            board.snapshots.push(ModuleSnapshotDef {
                name: name.clone(),
                drives: def.drives.clone(),
                ..ModuleSnapshotDef::default()
            });
        }
        ModuleChoiceDef {
            module: "Drive".into(),
            preset: PEDALBOARD.into(),
            snapshot: name,
        }
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
            let Some((_, amp)) = amp_map
                .iter()
                .find(|(old, _)| old.eq_ignore_ascii_case(&patch.preset))
            else {
                unmapped.push(patch.name.clone());
                continue;
            };
            let mut modules = vec![amp.clone()];
            modules.extend(board_pick.clone());
            let preset_name = format!("{} {stack}", def.name);
            if comp.preset(&preset_name).is_none() {
                comp.presets.push(RigPresetDef {
                    name: preset_name.clone(),
                    snapshots: Vec::new(),
                });
            }
            let preset = comp
                .presets
                .iter_mut()
                .find(|p| p.name.eq_ignore_ascii_case(&preset_name))
                .expect("just ensured");
            let snap = PresetSnapshotDef {
                blocks: Vec::new(),
                name: patch.name.clone(),
                modules,
                overrides: std::mem::take(&mut patch.overrides),
                level_db: 0.0,
                gain_bias_db: 0.0,
                macros: Vec::new(),
            };
            match preset
                .snapshots
                .iter_mut()
                .find(|s| s.name.eq_ignore_ascii_case(&patch.name))
            {
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
