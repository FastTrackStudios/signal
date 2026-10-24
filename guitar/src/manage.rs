//! Library management over the composition libraries: renaming, copying and
//! deleting module presets, their snapshots, block presets and presets, and
//! saving what the live patch plays back into them.
//!
//! Pure: everything here takes the libraries and the profiles as values, so
//! the rules — what follows a rename, what refuses a delete, what "save from
//! live" folds in — are tested without a rig. `session` locks, calls these,
//! saves what they touched and rebuilds.
//!
//! References are by **name** everywhere in the library (a patch's module
//! pick, a preset snapshot's, a Time snapshot's Delay pick, a block choice),
//! so a rename that did not follow them would silently unhook every user —
//! the preset would look renamed and every patch playing it would fall back
//! to nothing. Every rename here walks all of them; every delete refuses
//! while any is left, and says which.

use signal_proto::block::{BlockCategory, BlockType};

use crate::compose::{
    BlockChoiceDef, BlockPresetDef, Compositions, ModulePresetDef, ModuleSnapshotDef, ParamSetDef,
    PresetSnapshotDef, RigPresetDef,
};
use crate::profiles::{
    DriveSlotDef, ModuleChoiceDef, OverrideDef, PatchDef, PresetDef, ProfileDef, SongDef,
};

fn eq(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// A name that is not blank and not already taken (case-insensitively).
fn free<'a>(name: &str, mut taken: impl Iterator<Item = &'a str>) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("a blank name".to_string());
    }
    if taken.any(|t| eq(t, name)) {
        return Err(format!("'{name}' is taken"));
    }
    Ok(name.to_string())
}

/// How a patch is named in a "used by" list: with its profile, since the
/// same name can be a patch in two profiles.
fn patch_label(profile: &ProfileDef, patch: &PatchDef) -> String {
    if patch.song.is_empty() {
        format!("Patch {} ({})", patch.name, profile.name)
    } else {
        format!("Patch {} (song {})", patch.name, patch.song)
    }
}

/// Push unless already there — a song patch is attached to several profiles.
fn push_once(out: &mut Vec<String>, s: String) {
    if !out.contains(&s) {
        out.push(s);
    }
}

/// Whether a module pick names `module`'s preset `preset` (and, given one,
/// its snapshot `snapshot`: an empty pick means the first snapshot).
fn picks(c: &ModuleChoiceDef, module: &str, preset: &str, snapshot: Option<(&str, bool)>) -> bool {
    eq(&c.module, module)
        && eq(&c.preset, preset)
        && snapshot.is_none_or(|(s, first)| eq(&c.snapshot, s) || (c.snapshot.is_empty() && first))
}

// ── Used by ────────────────────────────────────────────────────────────────

/// Everything that refers to `module`'s preset `preset` — or, with
/// `snapshot`, to that one snapshot of it.
#[must_use]
pub fn module_users(
    comp: &Compositions,
    profiles: &[&ProfileDef],
    module: &str,
    preset: &str,
    snapshot: Option<&str>,
) -> Vec<String> {
    // An empty pick plays the first snapshot, so it refers to it.
    let first = comp
        .module(module, preset)
        .and_then(|m| m.snapshots.first())
        .is_some_and(|s| snapshot.is_some_and(|x| eq(&s.name, x)));
    let snap = snapshot.map(|s| (s, first));
    let mut out = Vec::new();
    for m in &comp.modules {
        if eq(&m.module, module) && eq(&m.name, preset) {
            continue;
        }
        if m.snapshots
            .iter()
            .any(|s| s.modules.iter().any(|c| picks(c, module, preset, snap)))
        {
            push_once(&mut out, format!("Module {} · {}", m.module, m.name));
        }
    }
    for p in &comp.presets {
        if p.snapshots
            .iter()
            .any(|s| s.modules.iter().any(|c| picks(c, module, preset, snap)))
        {
            push_once(&mut out, format!("Preset {}", p.name));
        }
    }
    for prof in profiles {
        for patch in &prof.patches {
            if patch.modules.iter().any(|c| picks(c, module, preset, snap)) {
                push_once(&mut out, patch_label(prof, patch));
            }
        }
    }
    out
}

/// Everything that puts block preset `name` on a block.
#[must_use]
pub fn block_users(comp: &Compositions, profiles: &[&ProfileDef], name: &str) -> Vec<String> {
    let uses = |list: &[BlockChoiceDef]| list.iter().any(|c| eq(&c.preset, name));
    let mut out = Vec::new();
    for m in &comp.modules {
        if m.snapshots.iter().any(|s| uses(&s.blocks)) {
            push_once(&mut out, format!("Module {} · {}", m.module, m.name));
        }
    }
    for p in &comp.presets {
        if p.snapshots.iter().any(|s| uses(&s.blocks)) {
            push_once(&mut out, format!("Preset {}", p.name));
        }
    }
    for prof in profiles {
        for patch in &prof.patches {
            if uses(&patch.blocks) {
                push_once(&mut out, patch_label(prof, patch));
            }
        }
    }
    out
}

/// The patches playing preset `name`.
#[must_use]
pub fn rig_preset_users(profiles: &[&ProfileDef], name: &str) -> Vec<String> {
    let mut out = Vec::new();
    for prof in profiles {
        for patch in &prof.patches {
            if eq(&patch.rig_preset, name) {
                push_once(&mut out, patch_label(prof, patch));
            }
        }
    }
    out
}

/// Refuse with the users, when there are any.
fn refuse_if_used(what: &str, users: &[String]) -> Result<(), String> {
    if users.is_empty() {
        return Ok(());
    }
    let shown: Vec<&str> = users.iter().take(3).map(String::as_str).collect();
    let more = users.len().saturating_sub(shown.len());
    Err(format!(
        "{what} is in use: {}{}",
        shown.join(", "),
        if more > 0 {
            format!(" and {more} more")
        } else {
            String::new()
        }
    ))
}

/// The indices of the profiles `touch` changed, for the caller to save.
fn each_profile(
    profiles: &mut [&mut ProfileDef],
    mut touch: impl FnMut(&mut PatchDef) -> bool,
) -> Vec<usize> {
    let mut touched = Vec::new();
    for (i, prof) in profiles.iter_mut().enumerate() {
        let mut any = false;
        for patch in &mut prof.patches {
            any |= touch(patch);
        }
        if any {
            touched.push(i);
        }
    }
    touched
}

// ── Module presets ─────────────────────────────────────────────────────────

fn module_mut<'a>(
    comp: &'a mut Compositions,
    module: &str,
    preset: &str,
) -> Result<&'a mut ModulePresetDef, String> {
    comp.modules
        .iter_mut()
        .find(|m| eq(&m.module, module) && eq(&m.name, preset))
        .ok_or_else(|| format!("no {module} preset '{preset}'"))
}

/// Rename a module preset; every pick of it follows. Returns the profiles
/// touched.
pub fn rename_module_preset(
    comp: &mut Compositions,
    profiles: &mut [&mut ProfileDef],
    module: &str,
    old: &str,
    new_name: &str,
) -> Result<Vec<usize>, String> {
    let new_name = free(
        new_name,
        comp.modules
            .iter()
            .filter(|m| eq(&m.module, module) && !eq(&m.name, old))
            .map(|m| m.name.as_str()),
    )?;
    module_mut(comp, module, old)?.name.clone_from(&new_name);
    let follow = |c: &mut ModuleChoiceDef| {
        if picks(c, module, old, None) {
            c.preset.clone_from(&new_name);
            true
        } else {
            false
        }
    };
    for m in &mut comp.modules {
        m.snapshots
            .iter_mut()
            .flat_map(|s| s.modules.iter_mut())
            .for_each(|c| {
                follow(c);
            });
    }
    for p in &mut comp.presets {
        p.snapshots
            .iter_mut()
            .flat_map(|s| s.modules.iter_mut())
            .for_each(|c| {
                follow(c);
            });
    }
    Ok(each_profile(profiles, |patch| {
        patch.modules.iter_mut().fold(false, |a, c| follow(c) | a)
    }))
}

/// Copy a module preset as `new_name`.
pub fn duplicate_module_preset(
    comp: &mut Compositions,
    module: &str,
    name: &str,
    new_name: &str,
) -> Result<(), String> {
    let new_name = free(
        new_name,
        comp.modules
            .iter()
            .filter(|m| eq(&m.module, module))
            .map(|m| m.name.as_str()),
    )?;
    let mut copy = module_mut(comp, module, name)?.clone();
    copy.name = new_name;
    comp.modules.push(copy);
    Ok(())
}

/// Delete a module preset, refused while anything refers to it.
pub fn delete_module_preset(
    comp: &mut Compositions,
    profiles: &[&ProfileDef],
    module: &str,
    name: &str,
) -> Result<(), String> {
    module_mut(comp, module, name)?;
    refuse_if_used(
        &format!("{module} preset '{name}'"),
        &module_users(comp, profiles, module, name, None),
    )?;
    comp.modules
        .retain(|m| !(eq(&m.module, module) && eq(&m.name, name)));
    Ok(())
}

/// Rename one snapshot of a module preset; picks naming it follow.
pub fn rename_module_snapshot(
    comp: &mut Compositions,
    profiles: &mut [&mut ProfileDef],
    module: &str,
    preset: &str,
    old: &str,
    new_name: &str,
) -> Result<Vec<usize>, String> {
    let m = module_mut(comp, module, preset)?;
    let new_name = free(
        new_name,
        m.snapshots
            .iter()
            .filter(|s| !eq(&s.name, old))
            .map(|s| s.name.as_str()),
    )?;
    let snap = m
        .snapshots
        .iter_mut()
        .find(|s| eq(&s.name, old))
        .ok_or_else(|| format!("no snapshot '{old}' in {module} · {preset}"))?;
    snap.name.clone_from(&new_name);
    // Only picks that spell the snapshot out: an empty pick means "the
    // first", which a rename does not change.
    let follow = |c: &mut ModuleChoiceDef| {
        if eq(&c.module, module) && eq(&c.preset, preset) && eq(&c.snapshot, old) {
            c.snapshot.clone_from(&new_name);
            true
        } else {
            false
        }
    };
    for m in &mut comp.modules {
        m.snapshots
            .iter_mut()
            .flat_map(|s| s.modules.iter_mut())
            .for_each(|c| {
                follow(c);
            });
    }
    for p in &mut comp.presets {
        p.snapshots
            .iter_mut()
            .flat_map(|s| s.modules.iter_mut())
            .for_each(|c| {
                follow(c);
            });
    }
    Ok(each_profile(profiles, |patch| {
        patch.modules.iter_mut().fold(false, |a, c| follow(c) | a)
    }))
}

/// Delete one snapshot — refused for the last, and while anything picks it.
pub fn delete_module_snapshot(
    comp: &mut Compositions,
    profiles: &[&ProfileDef],
    module: &str,
    preset: &str,
    snapshot: &str,
) -> Result<(), String> {
    let m = module_mut(comp, module, preset)?;
    if !m.snapshots.iter().any(|s| eq(&s.name, snapshot)) {
        return Err(format!("no snapshot '{snapshot}' in {module} · {preset}"));
    }
    if m.snapshots.len() <= 1 {
        return Err(format!(
            "'{snapshot}' is the only snapshot of {preset} — delete the preset instead"
        ));
    }
    refuse_if_used(
        &format!("snapshot '{snapshot}'"),
        &module_users(comp, profiles, module, preset, Some(snapshot)),
    )?;
    module_mut(comp, module, preset)?
        .snapshots
        .retain(|s| !eq(&s.name, snapshot));
    Ok(())
}

/// Whether `module` owns the chain block `name` of type `bt`, for deciding
/// which of a patch's edits a module's "save from live" takes
/// ([`crate::profiles::module_of_block`]): the Core owns its own blocks and
/// its Drive's and Amp's; Time owns no blocks of its own (its snapshots are
/// a Delay pick and a Reverb pick, whose modules own those). The pre
/// effects belong to no module.
fn owned_by(module: &str, name: &str, bt: BlockType) -> bool {
    let of = crate::profiles::module_of_block(name, bt);
    if module.eq_ignore_ascii_case(crate::profiles::CORE) {
        return of.is_some_and(|m| matches!(m, "Core" | "Drive" | "Amp"));
    }
    if module.eq_ignore_ascii_case("dynamics") {
        return bt.category() == BlockCategory::Dynamics && of != Some(crate::profiles::CORE);
    }
    of.is_some_and(|m| m.eq_ignore_ascii_case(module))
}

/// The chain blocks `pick` owns on a chain of `(name, type)`: the blocks of
/// its type, and any block its snapshot sets by name (an Amp snapshot's
/// `Amp EQ`). What a module's modified dot watches and its save folds in.
#[must_use]
pub fn owned_blocks(
    comp: &Compositions,
    pick: &ModuleChoiceDef,
    chain: &[(String, BlockType)],
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |b: &str| {
        if !b.is_empty() && !out.iter().any(|x| eq(x, b)) {
            out.push(b.to_string());
        }
    };
    for (name, bt) in chain {
        if owned_by(&pick.module, name, *bt) {
            add(name);
        }
    }
    if let Some(snap) = comp.module(&pick.module, &pick.preset).and_then(|m| {
        m.snapshots
            .iter()
            .find(|s| !pick.snapshot.is_empty() && eq(&s.name, &pick.snapshot))
            .or_else(|| m.snapshots.first())
    }) {
        snap.blocks.iter().for_each(|c| add(&c.block));
        snap.overrides.iter().for_each(|o| add(&o.block));
        // The slots it fills (a pedal swapped into one is its edit).
        snap.drives.iter().for_each(|d| add(&d.block));
    }
    out
}

/// Put `o` into `list`, replacing the override of the same block, parameter
/// and op.
fn put_override(list: &mut Vec<OverrideDef>, o: OverrideDef) {
    match list
        .iter_mut()
        .find(|x| eq(&x.block, &o.block) && x.op == o.op && eq(&x.param, &o.param))
    {
        Some(x) => *x = o,
        None => list.push(o),
    }
}

/// What saving a module from live needs to know about the live patch.
pub struct LiveModule<'a> {
    /// The module's effective pick on the patch (its preset snapshot's, or
    /// its own), if it has one.
    pub current: Option<&'a ModuleChoiceDef>,
    /// The blocks the module owns ([`owned_blocks`]).
    pub owned: &'a [String],
    /// The patch's effective picks of every module — a Time snapshot saves
    /// the Delay and Reverb that are playing.
    pub picks: &'a [ModuleChoiceDef],
}

/// Save what `module` plays on `patch` as snapshot `snapshot` of preset
/// `preset` (either created when new). The patch's own edits on the
/// module's blocks move into the snapshot, and the patch plays it.
pub fn save_module_snapshot(
    comp: &mut Compositions,
    patch: &mut PatchDef,
    live: &LiveModule<'_>,
    module: &str,
    preset: &str,
    snapshot: &str,
) -> Result<(), String> {
    let snapshot = snapshot.trim();
    let preset = preset.trim();
    if snapshot.is_empty() || preset.is_empty() {
        return Err("a blank name".to_string());
    }
    // Start from what plays now: the current pick's snapshot as saved.
    let mut snap: ModuleSnapshotDef = live
        .current
        .and_then(|c| {
            comp.module(&c.module, &c.preset).and_then(|m| {
                m.snapshots
                    .iter()
                    .find(|s| !c.snapshot.is_empty() && eq(&s.name, &c.snapshot))
                    .or_else(|| m.snapshots.first())
            })
        })
        .cloned()
        .unwrap_or_default();
    snap.name = snapshot.to_string();
    let owned = |b: &str| live.owned.iter().any(|x| eq(x, b));
    // The patch's block presets on those blocks first — a block preset
    // replaces what the snapshot dialled on that block, as it does when
    // picked — then its hand edits over them.
    for c in patch.blocks.iter().filter(|c| owned(&c.block)) {
        snap.overrides.retain(|o| !eq(&o.block, &c.block));
        match snap.blocks.iter_mut().find(|x| eq(&x.block, &c.block)) {
            Some(x) => *x = c.clone(),
            None => snap.blocks.push(c.clone()),
        }
    }
    for o in patch.overrides.iter().filter(|o| owned(&o.block)) {
        put_override(&mut snap.overrides, o.clone());
    }
    // The modules the snapshot plays (a Time snapshot's Delay and Reverb):
    // the ones playing now.
    for sub in &mut snap.modules {
        if let Some(p) = live.picks.iter().find(|p| eq(&p.module, &sub.module)) {
            *sub = p.clone();
        }
    }
    let subs: Vec<String> = snap.modules.iter().map(|m| m.module.clone()).collect();
    // Into the library.
    let canonical = match comp
        .modules
        .iter_mut()
        .find(|m| eq(&m.module, module) && eq(&m.name, preset))
    {
        Some(m) => {
            match m.snapshots.iter_mut().find(|s| eq(&s.name, snapshot)) {
                Some(s) => {
                    snap.name.clone_from(&s.name);
                    *s = snap.clone();
                }
                None => m.snapshots.push(snap.clone()),
            }
            m.name.clone()
        }
        None => {
            comp.modules.push(ModulePresetDef {
                module: live
                    .current
                    .map_or_else(|| module.to_string(), |c| c.module.clone()),
                name: preset.to_string(),
                snapshots: vec![snap.clone()],
            });
            preset.to_string()
        }
    };
    // The patch: its edits are the snapshot's now, and it plays that.
    patch.overrides.retain(|o| !owned(&o.block));
    patch.blocks.retain(|c| !owned(&c.block));
    let target = ModuleChoiceDef {
        module: live
            .current
            .map_or_else(|| module.to_string(), |c| c.module.clone()),
        preset: canonical,
        snapshot: snap.name.clone(),
    };
    let already = live.current.is_some_and(|c| {
        eq(&c.preset, &target.preset)
            && (eq(&c.snapshot, &target.snapshot)
                || (c.snapshot.is_empty()
                    && comp
                        .module(&target.module, &target.preset)
                        .and_then(|m| m.snapshots.first())
                        .is_some_and(|s| eq(&s.name, &target.snapshot))))
    });
    if !already {
        patch
            .modules
            .retain(|m| !subs.iter().any(|s| eq(s, &m.module)));
        match patch
            .modules
            .iter_mut()
            .find(|m| eq(&m.module, &target.module))
        {
            Some(m) => *m = target,
            None => patch.modules.push(target),
        }
    }
    Ok(())
}

/// Save the live patch's core tone as snapshot `snapshot` of Core preset
/// `preset` (each created when new; an existing snapshot is replaced — the
/// "update from live"), and play it on the patch.
///
/// A Core snapshot is its Drive and Amp module picks plus the core blocks'
/// presets, overrides and level. It starts from the Core the patch plays (if
/// any), takes the Drive and Amp it plays now, then the patch's own edits on
/// the core's blocks. A patch that plays no Amp module — its amp a profile
/// preset (`preset`, `preset2`) — gets one made from that preset, named for
/// it; one with no Drive module, a Drive snapshot of the board it plays
/// (`board`: the profile's slots, the patch's own over them) in Core
/// preset `{preset} Board`. So the Core carries the whole tone and plays the
/// same under any patch.
pub fn save_core_snapshot(
    comp: &mut Compositions,
    patch: &mut PatchDef,
    amps: &[PresetDef],
    board: &[DriveSlotDef],
    preset: &str,
    snapshot: &str,
) -> Result<(), String> {
    let (preset, snapshot) = (preset.trim(), snapshot.trim());
    if preset.is_empty() || snapshot.is_empty() {
        return Err("a blank name".to_string());
    }
    // What plays now: the Core snapshot as saved, if the patch has one.
    let mut snap: PresetSnapshotDef = comp
        .preset(&patch.rig_preset)
        .and_then(|p| {
            p.snapshots
                .iter()
                .find(|s| !patch.snapshot.is_empty() && eq(&s.name, &patch.snapshot))
                .or_else(|| p.snapshots.first())
        })
        .cloned()
        .unwrap_or_default();
    snap.name = snapshot.to_string();
    // The settings change: a frozen capture of the old ones is stale.
    snap.frozen = false;
    snap.frozen_nam.clear();
    snap.frozen_nam2.clear();
    let picks = crate::compose::module_picks(comp, patch);
    let pick_of = |m: &str| picks.iter().find(|p| eq(&p.module, m)).cloned();
    let amp = match pick_of("Amp") {
        Some(p) => Some(p),
        None => amp_module_from(comp, patch, amps),
    };
    let drive = match pick_of("Drive") {
        Some(p) => Some(p),
        None => drive_module_from(
            comp,
            board,
            &patch.drives,
            &format!("{preset} Board"),
            snapshot,
        ),
    };
    snap.modules = drive.into_iter().chain(amp).collect();
    // The patch's own block presets and edits on the core's blocks.
    let core = |b: &str| crate::compose::is_core_block(b);
    for c in patch.blocks.iter().filter(|c| core(&c.block)) {
        snap.overrides.retain(|o| !eq(&o.block, &c.block));
        match snap.blocks.iter_mut().find(|x| eq(&x.block, &c.block)) {
            Some(x) => *x = c.clone(),
            None => snap.blocks.push(c.clone()),
        }
    }
    for o in patch.overrides.iter().filter(|o| core(&o.block)) {
        put_override(&mut snap.overrides, o.clone());
    }
    // Into the library.
    let name = match comp.presets.iter_mut().find(|p| eq(&p.name, preset)) {
        Some(p) => {
            match p.snapshots.iter_mut().find(|s| eq(&s.name, snapshot)) {
                Some(s) => {
                    snap.name.clone_from(&s.name);
                    *s = snap.clone();
                }
                None => p.snapshots.push(snap.clone()),
            }
            p.name.clone()
        }
        None => {
            comp.presets.push(RigPresetDef {
                name: preset.to_string(),
                snapshots: vec![snap.clone()],
            });
            preset.to_string()
        }
    };
    crate::compose::put_core(patch, &name, &snap.name);
    Ok(())
}

/// An Amp module pick for a patch whose amp is a profile preset: the Amp
/// module preset named for it, made (or given this capture as a snapshot)
/// when the library has none playing it. `None` with no amp to take.
fn amp_module_from(
    comp: &mut Compositions,
    patch: &PatchDef,
    amps: &[PresetDef],
) -> Option<ModuleChoiceDef> {
    let left = amps.iter().find(|p| eq(&p.name, &patch.preset))?;
    let right = amps
        .iter()
        .find(|p| !patch.preset2.is_empty() && eq(&p.name, &patch.preset2));
    let snap = ModuleSnapshotDef {
        name: right.map_or_else(|| "Default".to_string(), |r| format!("+ {}", r.name)),
        nam: left.nam.clone(),
        cab: left.cab.clone(),
        level_db: left.level_db,
        nam2: right.map(|r| r.nam.clone()).unwrap_or_default(),
        cab2: right.map(|r| r.cab.clone()).unwrap_or_default(),
        level2_db: right.map_or(0.0, |r| r.level_db),
        ..ModuleSnapshotDef::default()
    };
    let same = |s: &ModuleSnapshotDef| {
        s.nam == snap.nam && s.cab == snap.cab && s.nam2 == snap.nam2 && s.cab2 == snap.cab2
    };
    let name = left.name.clone();
    let snapshot = match comp
        .modules
        .iter_mut()
        .find(|m| eq(&m.module, "Amp") && eq(&m.name, &name))
    {
        Some(m) => match m.snapshots.iter().find(|s| same(s)) {
            Some(s) => s.name.clone(),
            None => {
                let mut n = snap.name.clone();
                let mut i = 2;
                while m.snapshots.iter().any(|s| eq(&s.name, &n)) {
                    n = format!("{} {i}", snap.name);
                    i += 1;
                }
                m.snapshots.push(ModuleSnapshotDef {
                    name: n.clone(),
                    ..snap
                });
                n
            }
        },
        None => {
            let n = snap.name.clone();
            comp.modules.push(ModulePresetDef {
                module: "Amp".to_string(),
                name: name.clone(),
                snapshots: vec![snap],
            });
            n
        }
    };
    Some(ModuleChoiceDef {
        module: "Amp".to_string(),
        preset: name,
        snapshot,
    })
}

/// A Drive module pick for a patch that plays the profile's board: snapshot
/// `snapshot` of Drive preset `preset` holding the slots it plays (`board`,
/// the patch's `own` over them). `None` for an empty board.
fn drive_module_from(
    comp: &mut Compositions,
    board: &[DriveSlotDef],
    own: &[DriveSlotDef],
    preset: &str,
    snapshot: &str,
) -> Option<ModuleChoiceDef> {
    let mut drives: Vec<DriveSlotDef> = board.to_vec();
    for d in own {
        match drives.iter_mut().find(|x| eq(&x.block, &d.block)) {
            Some(x) => *x = d.clone(),
            None => drives.push(d.clone()),
        }
    }
    if drives.is_empty() {
        return None;
    }
    let snap = ModuleSnapshotDef {
        name: snapshot.to_string(),
        drives,
        ..ModuleSnapshotDef::default()
    };
    match comp
        .modules
        .iter_mut()
        .find(|m| eq(&m.module, "Drive") && eq(&m.name, preset))
    {
        Some(m) => match m.snapshots.iter_mut().find(|s| eq(&s.name, snapshot)) {
            Some(s) => *s = snap,
            None => m.snapshots.push(snap),
        },
        None => comp.modules.push(ModulePresetDef {
            module: "Drive".to_string(),
            name: preset.to_string(),
            snapshots: vec![snap],
        }),
    }
    Some(ModuleChoiceDef {
        module: "Drive".to_string(),
        preset: preset.to_string(),
        snapshot: snapshot.to_string(),
    })
}

/// Drop the patch's own edits on `owned` blocks — back to what the modules
/// and preset say. Returns whether anything changed.
pub fn revert_blocks(patch: &mut PatchDef, owned: &[String]) -> bool {
    let before = patch.overrides.len() + patch.blocks.len() + patch.drives.len();
    let owned = |b: &str| owned.iter().any(|x| eq(x, b));
    patch.overrides.retain(|o| !owned(&o.block));
    patch.blocks.retain(|c| !owned(&c.block));
    // A pedal or capture the patch picked in a module's slot is an edit too.
    patch.drives.retain(|d| !owned(&d.block));
    before != patch.overrides.len() + patch.blocks.len() + patch.drives.len()
}

// ── Block presets ──────────────────────────────────────────────────────────

/// The live block a block preset is saved from.
pub struct LiveBlockState<'a> {
    pub name: &'a str,
    /// Its type's storage key (`delay`).
    pub block_type: &'a str,
    pub params: Vec<(String, f32)>,
    pub bypassed: bool,
    /// The block preset it plays now (effective), if any.
    pub current: Option<&'a str>,
}

/// Save the live block as block preset `name` — new, or replacing that
/// preset's settings. The patch's hand edits on the block go, and it plays
/// the preset.
pub fn save_block_preset(
    comp: &mut Compositions,
    patch: &mut PatchDef,
    live: &LiveBlockState<'_>,
    name: &str,
) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("a blank name".to_string());
    }
    let params: Vec<ParamSetDef> = live
        .params
        .iter()
        .map(|(p, v)| ParamSetDef {
            param: p.clone(),
            value: *v,
        })
        .collect();
    let canonical = match comp.blocks.iter_mut().find(|b| eq(&b.name, name)) {
        Some(b) if !eq(&b.block_type, live.block_type) => {
            return Err(format!(
                "'{}' is a {} preset, not a {} one",
                b.name, b.block_type, live.block_type
            ));
        }
        Some(b) => {
            b.params = params;
            b.bypass = live.bypassed;
            b.name.clone()
        }
        None => {
            comp.blocks.push(BlockPresetDef {
                block_type: live.block_type.to_string(),
                name: name.to_string(),
                params,
                bypass: live.bypassed,
                target_gr_db: 0.0,
                macros: Vec::new(),
            });
            name.to_string()
        }
    };
    patch.overrides.retain(|o| !eq(&o.block, live.name));
    if !live.current.is_some_and(|c| eq(c, &canonical)) {
        let choice = BlockChoiceDef {
            block: live.name.to_string(),
            preset: canonical,
        };
        match patch.blocks.iter_mut().find(|b| eq(&b.block, live.name)) {
            Some(b) => *b = choice,
            None => patch.blocks.push(choice),
        }
    }
    Ok(())
}

/// Rename a block preset; every choice of it follows.
pub fn rename_block_preset(
    comp: &mut Compositions,
    profiles: &mut [&mut ProfileDef],
    old: &str,
    new_name: &str,
) -> Result<Vec<usize>, String> {
    let new_name = free(
        new_name,
        comp.blocks
            .iter()
            .filter(|b| !eq(&b.name, old))
            .map(|b| b.name.as_str()),
    )?;
    comp.blocks
        .iter_mut()
        .find(|b| eq(&b.name, old))
        .ok_or_else(|| format!("no block preset '{old}'"))?
        .name
        .clone_from(&new_name);
    let follow = |c: &mut BlockChoiceDef| {
        if eq(&c.preset, old) {
            c.preset.clone_from(&new_name);
            true
        } else {
            false
        }
    };
    for m in &mut comp.modules {
        m.snapshots
            .iter_mut()
            .flat_map(|s| s.blocks.iter_mut())
            .for_each(|c| {
                follow(c);
            });
    }
    for p in &mut comp.presets {
        p.snapshots
            .iter_mut()
            .flat_map(|s| s.blocks.iter_mut())
            .for_each(|c| {
                follow(c);
            });
    }
    Ok(each_profile(profiles, |patch| {
        patch.blocks.iter_mut().fold(false, |a, c| follow(c) | a)
    }))
}

/// Copy a block preset as `new_name`.
pub fn duplicate_block_preset(
    comp: &mut Compositions,
    name: &str,
    new_name: &str,
) -> Result<(), String> {
    let new_name = free(new_name, comp.blocks.iter().map(|b| b.name.as_str()))?;
    let mut copy = comp
        .blocks
        .iter()
        .find(|b| eq(&b.name, name))
        .cloned()
        .ok_or_else(|| format!("no block preset '{name}'"))?;
    copy.name = new_name;
    comp.blocks.push(copy);
    Ok(())
}

/// Delete a block preset, refused while anything puts it on a block.
pub fn delete_block_preset(
    comp: &mut Compositions,
    profiles: &[&ProfileDef],
    name: &str,
) -> Result<(), String> {
    if !comp.blocks.iter().any(|b| eq(&b.name, name)) {
        return Err(format!("no block preset '{name}'"));
    }
    refuse_if_used(
        &format!("block preset '{name}'"),
        &block_users(comp, profiles, name),
    )?;
    comp.blocks.retain(|b| !eq(&b.name, name));
    Ok(())
}

// ── Presets (compositions) ─────────────────────────────────────────────────

/// Rename a preset; the patches playing it follow.
pub fn rename_rig_preset(
    comp: &mut Compositions,
    profiles: &mut [&mut ProfileDef],
    old: &str,
    new_name: &str,
) -> Result<Vec<usize>, String> {
    let new_name = free(
        new_name,
        comp.presets
            .iter()
            .filter(|p| !eq(&p.name, old))
            .map(|p| p.name.as_str()),
    )?;
    comp.presets
        .iter_mut()
        .find(|p| eq(&p.name, old))
        .ok_or_else(|| format!("no preset '{old}'"))?
        .name
        .clone_from(&new_name);
    Ok(each_profile(profiles, |patch| {
        if eq(&patch.rig_preset, old) {
            patch.rig_preset.clone_from(&new_name);
            true
        } else {
            false
        }
    }))
}

/// Copy a preset as `new_name`.
pub fn duplicate_rig_preset(
    comp: &mut Compositions,
    name: &str,
    new_name: &str,
) -> Result<(), String> {
    let new_name = free(new_name, comp.presets.iter().map(|p| p.name.as_str()))?;
    let mut copy = comp
        .preset(name)
        .cloned()
        .ok_or_else(|| format!("no preset '{name}'"))?;
    copy.name = new_name;
    comp.presets.push(copy);
    Ok(())
}

/// Delete a preset, refused while a patch plays it.
pub fn delete_rig_preset(
    comp: &mut Compositions,
    profiles: &[&ProfileDef],
    name: &str,
) -> Result<(), String> {
    if comp.preset(name).is_none() {
        return Err(format!("no preset '{name}'"));
    }
    refuse_if_used(
        &format!("preset '{name}'"),
        &rig_preset_users(profiles, name),
    )?;
    comp.presets.retain(|p| !eq(&p.name, name));
    Ok(())
}

// ── Presets (`tones.styx`) ────────────────────────────────────────────────

/// The time modules: a Time pick covers its Delay and Reverb.
fn is_time_module(m: &str) -> bool {
    ["Time", "Delay", "Reverb"].iter().any(|t| eq(t, m))
}

/// Play preset `name` on `patch`: the patch names it, and gives up its own
/// picks and edits on what the preset covers — its Core (with the Drive and
/// Amp picked over it and the edits on the core's blocks) when the preset
/// names one, its Time when the preset picks one, the blocks the preset
/// sets — so the preset is heard; what the preset leaves alone stays the
/// patch's.
pub fn put_tone(comp: &Compositions, patch: &mut PatchDef, name: &str) -> Result<(), String> {
    let t = comp
        .tone(name)
        .ok_or_else(|| format!("no preset '{name}'"))?;
    patch.tone.clone_from(&t.name);
    if !t.rig_preset.is_empty() {
        patch.rig_preset.clear();
        patch.snapshot.clear();
        patch
            .modules
            .retain(|m| !eq(&m.module, "Drive") && !eq(&m.module, "Amp"));
        patch
            .blocks
            .retain(|c| !crate::compose::is_core_block(&c.block));
        patch
            .overrides
            .retain(|o| !crate::compose::is_core_block(&o.block));
    }
    if !t.preset.is_empty() {
        patch.preset.clear();
        patch.preset2.clear();
    }
    let picks_time = t.modules.iter().any(|m| is_time_module(&m.module));
    patch.modules.retain(|m| {
        !t.modules.iter().any(|x| eq(&x.module, &m.module))
            && !(picks_time && is_time_module(&m.module))
    });
    let time_block = crate::compose::is_time_block;
    let covered = |b: &str| {
        t.blocks.iter().any(|c| eq(&c.block, b))
            || t.overrides.iter().any(|o| eq(&o.block, b))
            || (picks_time && time_block(b))
    };
    patch.blocks.retain(|c| !covered(&c.block));
    patch.overrides.retain(|o| !covered(&o.block));
    patch
        .drives
        .retain(|d| !t.drives.iter().any(|x| eq(&x.block, &d.block)));
    Ok(())
}

/// Save what `patch` plays as preset `name` — new, or replacing that
/// preset ("update from the patch") — and play it: the patch's whole sound
/// (its Core or amp, its module picks, block presets, drive slots, edits,
/// level and macro positions, with any preset it played under them) moves
/// into the preset, and the patch names it. Patches already naming the
/// preset hear the update.
pub fn save_tone(comp: &mut Compositions, patch: &mut PatchDef, name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("a blank name".to_string());
    }
    let tone = tone_from(comp, patch, name);
    let tone_name = tone.name.clone();
    match comp.tones.iter_mut().find(|t| eq(&t.name, &tone_name)) {
        Some(t) => *t = tone,
        None => comp.tones.push(tone),
    }
    // The patch: nothing of its own left — it plays the preset.
    patch.tone = tone_name;
    patch.rig_preset.clear();
    patch.snapshot.clear();
    if !comp.tone(&patch.tone).is_some_and(|t| t.preset.is_empty()) {
        patch.preset.clear();
        patch.preset2.clear();
    }
    patch.modules.clear();
    patch.blocks.clear();
    patch.drives.clear();
    patch.overrides.clear();
    patch.level_db = 0.0;
    patch.macros.clear();
    Ok(())
}

/// What `patch` plays, as preset `name` (the patch untouched): its Core or
/// amp, module picks, block presets, drive slots, edits, level and macro
/// positions, with any preset it plays under them.
#[must_use]
pub fn tone_from(comp: &Compositions, patch: &PatchDef, name: &str) -> crate::compose::ToneDef {
    let p = crate::compose::with_tone(comp, patch);
    crate::compose::ToneDef {
        name: comp
            .tone(name)
            .map_or_else(|| name.trim().to_string(), |t| t.name.clone()),
        rig_preset: p.rig_preset.clone(),
        snapshot: p.snapshot.clone(),
        preset: if p.rig_preset.is_empty() {
            p.preset.clone()
        } else {
            String::new()
        },
        preset2: if p.rig_preset.is_empty() {
            p.preset2.clone()
        } else {
            String::new()
        },
        modules: p.modules.clone(),
        blocks: p.blocks.clone(),
        drives: p.drives.clone(),
        overrides: p.overrides.clone(),
        level_db: p.level_db,
        macros: p.macros.clone(),
    }
}

/// Presets from patches: one per patch that plays none, named after it and
/// holding the sound it plays — skipping a name a preset already has. The
/// patches are not changed. Returns the names made.
pub fn tones_from_patches(comp: &mut Compositions, patches: &[PatchDef]) -> Vec<String> {
    let mut made = Vec::new();
    for p in patches {
        let name = p.name.trim();
        if name.is_empty()
            || p.name == crate::profiles::PRESET_BENCH
            || !p.tone.is_empty()
            || comp.tone(name).is_some()
        {
            continue;
        }
        let t = tone_from(comp, p, name);
        made.push(t.name.clone());
        comp.tones.push(t);
    }
    made
}

/// The patches playing preset `name`, by label.
fn tone_users(profiles: &[&ProfileDef], name: &str) -> Vec<String> {
    let mut out = Vec::new();
    for prof in profiles {
        for patch in prof.patches.iter().filter(|p| eq(&p.tone, name)) {
            push_once(&mut out, patch_label(prof, patch));
        }
    }
    out
}

/// Rename a preset; the patches naming it follow.
pub fn rename_tone(
    comp: &mut Compositions,
    profiles: &mut [&mut ProfileDef],
    old: &str,
    new_name: &str,
) -> Result<Vec<usize>, String> {
    let new_name = free(
        new_name,
        comp.tones
            .iter()
            .filter(|t| !eq(&t.name, old))
            .map(|t| t.name.as_str()),
    )?;
    comp.tones
        .iter_mut()
        .find(|t| eq(&t.name, old))
        .ok_or_else(|| format!("no preset '{old}'"))?
        .name
        .clone_from(&new_name);
    Ok(each_profile(profiles, |patch| {
        if eq(&patch.tone, old) {
            patch.tone.clone_from(&new_name);
            true
        } else {
            false
        }
    }))
}

/// Copy a preset as `new_name`.
pub fn duplicate_tone(comp: &mut Compositions, name: &str, new_name: &str) -> Result<(), String> {
    let new_name = free(new_name, comp.tones.iter().map(|t| t.name.as_str()))?;
    let mut copy = comp
        .tone(name)
        .cloned()
        .ok_or_else(|| format!("no preset '{name}'"))?;
    copy.name = new_name;
    comp.tones.push(copy);
    Ok(())
}

/// Delete a preset — refused while a patch plays it.
pub fn delete_tone(
    comp: &mut Compositions,
    profiles: &[&ProfileDef],
    name: &str,
) -> Result<(), String> {
    if comp.tone(name).is_none() {
        return Err(format!("no preset '{name}'"));
    }
    refuse_if_used(&format!("preset '{name}'"), &tone_users(profiles, name))?;
    comp.tones.retain(|t| !eq(&t.name, name));
    Ok(())
}

// ── Songs ──────────────────────────────────────────────────────────────────

/// A copy of song `name` as `new_name`: sections, recalls, switch tuning and
/// jobs. Its own patches are copied too, renamed `<patch> (<new song>)` —
/// patch names are unique in a profile, and a song's patches join every
/// profile it plays on — with every recall and tuning re-pointed at the
/// copies, so editing one song's sound never changes the other's.
pub fn duplicate_song(
    songs: &[SongDef],
    taken_patches: &[&str],
    name: &str,
    new_name: &str,
) -> Result<SongDef, String> {
    let new_name = free(new_name, songs.iter().map(|s| s.name.as_str()))?;
    let mut copy = songs
        .iter()
        .find(|s| eq(&s.name, name))
        .cloned()
        .ok_or_else(|| format!("no song '{name}'"))?;
    copy.name.clone_from(&new_name);
    let mut renames: Vec<(String, String)> = Vec::new();
    for p in &mut copy.patches {
        let base = format!("{} ({new_name})", p.name);
        let mut fresh = base.clone();
        let mut n = 2;
        while taken_patches.iter().any(|t| eq(t, &fresh))
            || renames.iter().any(|(_, r)| eq(r, &fresh))
        {
            fresh = format!("{base} {n}");
            n += 1;
        }
        renames.push((p.name.clone(), fresh.clone()));
        p.name = fresh;
        p.song.clone_from(&new_name);
    }
    let follow = |s: &mut String| {
        if let Some((_, new)) = renames.iter().find(|(old, _)| eq(old, s)) {
            s.clone_from(new);
        }
    };
    for r in &mut copy.part_recalls {
        follow(&mut r.patch);
    }
    for d in &mut copy.stack_defaults {
        follow(&mut d.patch);
        d.patches.iter_mut().for_each(follow);
    }
    Ok(copy)
}

/// Move the item at `from` to `to`. `false` when either is out of range.
pub fn move_item<T>(list: &mut Vec<T>, from: usize, to: usize) -> bool {
    if from >= list.len() || to >= list.len() {
        return false;
    }
    let item = list.remove(from);
    list.insert(to, item);
    true
}

/// Where a cursor on index `cur` lands after moving `from` to `to`, so it
/// stays on the same item.
#[must_use]
pub const fn follow_move(cur: usize, from: usize, to: usize) -> usize {
    if cur == from {
        to
    } else if from < cur && to >= cur {
        cur - 1
    } else if from > cur && to <= cur {
        cur + 1
    } else {
        cur
    }
}
