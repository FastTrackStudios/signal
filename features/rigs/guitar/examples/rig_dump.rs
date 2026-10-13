//! Lay a shipped profile's patch out the way the running rig presents it to
//! its UI — the live block list — and write it as JSON, without an engine.
//!
//! ```text
//! cargo run -p signal-guitar --example rig_dump -- <profile> <patch|""> <out.json>
//! ```
//!
//! `<profile>` is a shipped profile (`blues`, `worship`); an empty patch name
//! means the profile's default patch (its first when it names none). Another
//! app reads the file to draw the rig: every block with a backend in chain
//! order, its bypass, the preset the board names it by, its capture, its
//! Output Level and its params; plus the block presets the patch picks (the
//! nameplate text).
//!
//! The recipe is the live view's (`GuitarRigBackend::resync_blocks` then
//! `label_board`), fed from the built chain with its real bypass state.

use std::collections::BTreeMap;
use std::path::Path;

use facet::Facet;
use guitar::compose::{
    self, BlockLib, Compositions, ModuleLib, ModuleSnapshotDef, PresetLib, ToneLib,
};
use guitar::library::DrivePresetLib;
use guitar::profiles::{self, DrivePresetDef, PatchDef, ProfileDef};
use guitar::session::param_specs;
use signal_proto::BlockType;
use sampler::rig::RigBlock;

const MODULES: &str = include_str!("../default-config/modules.styx");
const PRESETS: &str = include_str!("../default-config/presets.styx");
const BLOCKS: &str = include_str!("../default-config/blocks.styx");
const DRIVES: &str = include_str!("../default-config/drive-presets.styx");
const TONES: &str = include_str!("../default-config/tones.styx");
const BLUES: &str = include_str!("../default-config/profiles/blues.styx");
const WORSHIP: &str = include_str!("../default-config/profiles/worship.styx");

#[derive(Facet)]
struct Dump {
    profile: String,
    patch: String,
    blocks: Vec<ChainBlock>,
    active_blocks: BTreeMap<String, String>,
}

#[derive(Facet)]
struct ChainBlock {
    name: String,
    block_type: String,
    module: String,
    bypassed: bool,
    preset: String,
    detail: String,
    option: Option<u32>,
    options: Vec<String>,
    asset: String,
    empty: bool,
    output_level_db: Option<f32>,
    params: Vec<Param>,
}

#[derive(Facet)]
struct Param {
    name: String,
    value: f32,
}

fn parse<T: for<'a> Facet<'a>>(what: &str, text: &str) -> Result<T, String> {
    facet_styx::from_str(text).map_err(|e| format!("{what}: {e}"))
}

fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn file_stem(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn u32_of(i: usize) -> u32 {
    u32::try_from(i).unwrap_or(u32::MAX)
}

/// The patch's Amp module pick: its module preset's name and the snapshot
/// it plays (the first when the pick names none that exists).
fn amp_pick<'c>(
    comp: &'c Compositions,
    own: &PatchDef,
) -> Option<(String, Vec<String>, usize, &'c ModuleSnapshotDef)> {
    let pick = compose::module_picks(comp, own)
        .into_iter()
        .find(|m| m.module.eq_ignore_ascii_case("Amp"))?;
    let module = comp.module("Amp", &pick.preset)?;
    let snaps: Vec<String> = module.snapshots.iter().map(|s| s.name.clone()).collect();
    let idx = snaps
        .iter()
        .position(|s| s.eq_ignore_ascii_case(&pick.snapshot))
        .unwrap_or(0);
    let snap = module.snapshots.get(idx)?;
    Some((module.name.clone(), snaps, idx, snap))
}

/// The Output Level the live rig reports for a NAM block (the session's
/// `level_home` + `stored_level`).
fn output_level(
    comp: &Compositions,
    def: &ProfileDef,
    own: &PatchDef,
    drives: &[DrivePresetDef],
    block: &RigBlock,
) -> Option<f32> {
    if block.nam.is_empty() {
        return None;
    }
    match block.block_type {
        BlockType::Drive | BlockType::Boost => {
            let file = |p: &str| Path::new(p).file_name().map(std::ffi::OsStr::to_owned);
            Some(
                drives
                    .iter()
                    .flat_map(|p| &p.options)
                    .find(|o| {
                        o.nam == block.nam
                            || (file(&o.nam).is_some() && file(&o.nam) == file(&block.nam))
                    })
                    .map_or(0.0, |o| o.level_db),
            )
        }
        BlockType::Amp => {
            let second = block.name.eq_ignore_ascii_case("Amp R");
            if own.rig_preset.is_empty() {
                let pool = if second { &own.preset2 } else { &own.preset };
                return def
                    .presets
                    .iter()
                    .find(|p| p.name.eq_ignore_ascii_case(pool))
                    .map(|p| p.level_db);
            }
            let pick = compose::module_picks(comp, own)
                .into_iter()
                .find(|m| m.module.eq_ignore_ascii_case("Amp"))?;
            comp.module("Amp", &pick.preset)?
                .snapshots
                .iter()
                .find(|s| s.name.eq_ignore_ascii_case(&pick.snapshot))
                .map(|s| if second { s.level2_db } else { s.level_db })
        }
        _ => None,
    }
}

/// The pool preset an amp plays on a patch that composes none: the
/// patch's first (Amp L) or second (Amp R).
fn pool_name(own: &PatchDef, right: bool) -> String {
    if right {
        own.preset2.clone()
    } else {
        own.preset.clone()
    }
}

/// What the board names a block by: its preset, the alternatives it steps
/// through and which one plays (the session's `resync_blocks`).
fn named_by(
    comp: &Compositions,
    def: &ProfileDef,
    own: &PatchDef,
    drives: &[DrivePresetDef],
    block: &RigBlock,
    name: &str,
) -> (String, Vec<String>, Option<u32>) {
    let right = name.eq_ignore_ascii_case("Amp R");
    if let Some(d) = def
        .drives
        .iter()
        .find(|d| d.block.eq_ignore_ascii_case(name))
    {
        return drives
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(&d.preset))
            .map(|p| {
                (
                    p.name.clone(),
                    p.options.iter().map(|o| o.name.clone()).collect(),
                    Some(u32_of(d.option)),
                )
            })
            .unwrap_or_default();
    }
    if block.block_type == BlockType::Amp {
        match amp_pick(comp, own) {
            Some((_, _, _, snap)) if right && snap.nam2.is_empty() => {
                (String::new(), Vec::new(), None)
            }
            Some((module, snaps, idx, _)) => {
                let label = format!("{module} · {}", snaps.get(idx).cloned().unwrap_or_default());
                (label, snaps, Some(u32_of(idx)))
            }
            None => {
                let current = pool_name(own, right);
                let pool: Vec<String> = def.presets.iter().map(|p| p.name.clone()).collect();
                let index = pool
                    .iter()
                    .position(|p| p.eq_ignore_ascii_case(&current))
                    .unwrap_or(0);
                (current, pool, Some(u32_of(index)))
            }
        }
    } else if block.block_type == BlockType::Cabinet {
        (file_stem(&block.ir), Vec::new(), None)
    } else {
        compose::block_picks(comp, own)
            .into_iter()
            .find(|c| c.block.eq_ignore_ascii_case(name))
            .map(|c| (c.preset, Vec::new(), None))
            .unwrap_or_default()
    }
}

/// One block as the live view lays it out, before the board labels its
/// drive slots.
fn live_block(
    comp: &Compositions,
    def: &ProfileDef,
    own: &PatchDef,
    drives: &[DrivePresetDef],
    block: &RigBlock,
) -> ChainBlock {
    let name = if block.name.trim().is_empty() {
        format!("{:?}", block.block_type)
    } else {
        block.name.clone()
    };
    let params = param_specs(block.block_type)
        .into_iter()
        .map(|(pname, _, _, dflt)| Param {
            value: block.param_f32(&pname).unwrap_or(dflt),
            name: pname,
        })
        .collect();
    let right = name.eq_ignore_ascii_case("Amp R");
    let (preset, options, option) = named_by(comp, def, own, drives, block, &name);
    let asset = if block.block_type == BlockType::Amp {
        let composed = amp_pick(comp, own).map(|(_, _, _, snap)| {
            if right {
                snap.nam2.clone()
            } else {
                snap.nam.clone()
            }
        });
        let path = composed.filter(|n| !n.is_empty()).unwrap_or_else(|| {
            let preset = pool_name(own, right);
            def.presets
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(&preset))
                .map(|p| p.nam.clone())
                .unwrap_or_default()
        });
        file_name(&path)
    } else {
        String::new()
    };
    ChainBlock {
        output_level_db: output_level(comp, def, own, drives, block),
        block_type: block.block_type.as_str().to_string(),
        module: block.module.clone(),
        bypassed: block.bypassed,
        detail: String::new(),
        empty: false,
        name,
        preset,
        option,
        options,
        asset,
        params,
    }
}

/// The board's Drive/Boost blocks by position — Boost, Drive 1-3 — named by
/// the pedal and capture each slot plays (the session's `label_board`).
fn label_board(
    comp: &Compositions,
    def: &ProfileDef,
    own: &PatchDef,
    drives: &[DrivePresetDef],
    blocks: &mut [ChainBlock],
) {
    let mut one = def.clone();
    one.patches = vec![own.clone()];
    let flat = compose::flatten(&one, comp);
    let slots = flat
        .patches
        .first()
        .map_or_else(|| def.drives.clone(), |p| compose::drives_for(&flat, p));
    let board = blocks
        .iter_mut()
        .filter(|b| {
            b.block_type == BlockType::Drive.as_str() || b.block_type == BlockType::Boost.as_str()
        })
        .zip(profiles::BOARD_SLOTS);
    for (b, slot) in board {
        let sp = profiles::slot_pedal(slot, &slots, drives);
        if let Some(p) = drives
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(&sp.pedal))
        {
            b.options = p.options.iter().map(|o| o.name.clone()).collect();
            b.option = Some(u32_of(
                p.options
                    .iter()
                    .position(|o| o.name == sp.option)
                    .unwrap_or(0),
            ));
        }
        b.preset = sp.pedal;
        b.detail = sp.option;
        b.asset = file_name(&sp.nam);
        b.empty = sp.empty;
    }
}

fn dump(profile: &str, patch: &str) -> Result<Dump, String> {
    let def: ProfileDef = match profile.to_ascii_lowercase().as_str() {
        "blues" => parse("profiles/blues.styx", BLUES)?,
        "worship" => parse("profiles/worship.styx", WORSHIP)?,
        other => return Err(format!("no shipped profile `{other}` (blues, worship)")),
    };
    let comp = Compositions {
        modules: parse::<ModuleLib>("modules.styx", MODULES)?.presets,
        presets: parse::<PresetLib>("presets.styx", PRESETS)?.presets,
        blocks: parse::<BlockLib>("blocks.styx", BLOCKS)?.presets,
        tones: parse::<ToneLib>("tones.styx", TONES)?.tones,
    };
    let drives = parse::<DrivePresetLib>("drive-presets.styx", DRIVES)?.presets;
    let built = profiles::build_profile(&compose::flatten(&def, &comp), &drives);
    let wanted = if patch.is_empty() {
        def.default_patch.as_str()
    } else {
        patch
    };
    let rig_patch = if wanted.is_empty() {
        built.patches.first()
    } else {
        built
            .patches
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(wanted))
    }
    .ok_or_else(|| format!("{}: no patch `{wanted}`", def.name))?;
    let own = def
        .patches
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(&rig_patch.name))
        .ok_or_else(|| format!("{}: `{}` has no definition", def.name, rig_patch.name))?;
    let mut blocks: Vec<ChainBlock> = rig_patch
        .chain
        .iter()
        .filter(|b| b.has_backend())
        .map(|b| live_block(&comp, &def, own, &drives, b))
        .collect();
    label_board(&comp, &def, own, &drives, &mut blocks);
    let active_blocks = compose::block_picks(&comp, own)
        .into_iter()
        .map(|c| (c.block, c.preset))
        .collect();
    Ok(Dump {
        profile: def.name.clone(),
        patch: rig_patch.name.clone(),
        blocks,
        active_blocks,
    })
}

fn run() -> Result<String, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [profile, patch, out] = args.as_slice() else {
        return Err("usage: rig_dump <profile> <patch|\"\"> <out.json>".into());
    };
    let dump = dump(profile, patch)?;
    let json = facet_json::to_string_pretty(&dump).map_err(|e| format!("json: {e}"))?;
    std::fs::write(out, json).map_err(|e| format!("{out}: {e}"))?;
    Ok(format!(
        "{} / {}: {} blocks → {out}",
        dump.profile,
        dump.patch,
        dump.blocks.len()
    ))
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(line) => {
            println!("{line}");
            std::process::ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("rig_dump: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
