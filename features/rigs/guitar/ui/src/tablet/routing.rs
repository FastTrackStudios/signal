//! Routing — the patch playing on the rig's routing grid (`grid::RigGraph`,
//! signal-grid-ui's `RigGridPanel`): its modules and blocks as the rig
//! resolves them. A tap selects a block or a whole module: the FX row below
//! shows its controls, and the browser beside it opens on it — its kind,
//! and the preset it plays.

use dioxus::prelude::*;
use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{CompositionModel, LiveBlock};

use super::routing_canvas::{CanvasCell, CanvasItem, CanvasModule, CanvasPick, CanvasSel};
use super::tokens::*;
use signal_proto::block::BlockType;
use crate::state::RigViewState;

/// What is selected in the routing grid.
#[derive(Clone, PartialEq, Debug)]
pub enum Selected {
    /// A block, by its live id.
    Block(String),
    /// A module: its name and the ids of the blocks in it.
    Module(String, Vec<String>),
}

/// The routing selection, shared with the FX row and the browser.
#[derive(Clone, Copy)]
pub struct RoutingSel(pub Signal<Option<Selected>>);

/// Where the browser goes for a selection: its kind (`block:delay`,
/// `module:Amp`) and, inside it, the preset (and variation) in use.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Focus {
    pub kind: String,
    pub preset: String,
    pub variation: String,
    /// Open on the search, the keyboard up.
    pub search: bool,
}

/// The browser's focus, set by a selection here.
#[derive(Clone, Copy)]
pub struct BrowserFocus(pub Signal<Option<Focus>>);

/// The browser's place for a selection: a block's presets when its type has
/// any (on its preset), else its module's (on the preset · variation the
/// patch plays there).
pub fn focus_for(sel: &Selected, blocks: &[LiveBlock], comp: &CompositionModel) -> Option<Focus> {
    let module_focus = |m: &str| {
        let kind = ["Core", "Amp", "Drive", "Time", "Delay", "Reverb"].iter().find(|k| k.eq_ignore_ascii_case(m))?;
        let pick = comp.active_modules.iter().find(|p| p.module.eq_ignore_ascii_case(m));
        Some(Focus {
            kind: format!("module:{kind}"),
            preset: pick.map(|p| p.preset.clone()).unwrap_or_default(),
            variation: pick.map(|p| p.snapshot.clone()).unwrap_or_default(),
            search: false,
        })
    };
    match sel {
        Selected::Module(m, _) => module_focus(m),
        Selected::Block(id) => {
            let b = blocks.iter().find(|b| b.id == *id)?;
            let t = b.block_type.as_str().to_lowercase();
            if comp.block_presets.iter().any(|p| p.block_type.eq_ignore_ascii_case(&t)) {
                return Some(Focus { kind: format!("block:{t}"), preset: b.preset.clone(), variation: String::new(), search: false });
            }
            module_focus(&b.module)
        }
    }
}

#[component]
pub fn Routing(state: RigViewState) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let sel = use_context::<RoutingSel>().0;
    let focus = try_use_context::<BrowserFocus>();
    let mut comp = use_signal(CompositionModel::default);
    // The modules the patch plays, fetched again when it changes.
    let rig_fetch = rig.clone();
    use_effect(move || {
        let _ = state.blocks.read();
        let _ = state.active_patch.read();
        if let Some(r) = rig_fetch.clone() {
            spawn(async move {
                if let Ok(c) = r.compositions().await {
                    comp.set(c);
                }
            });
        }
    });
    let blocks = state.blocks.read().clone();
    let faces = crate::rig_faces::use_faces();
    let modules = canvas_modules(&blocks, &faces);
    let selected = sel().map(|s| match s {
        Selected::Block(id) => CanvasSel::Block(id),
        Selected::Module(m, _) => CanvasSel::Module(m),
    });
    let on_pick = move |pick: CanvasPick| {
        let blocks = state.blocks.peek().clone();
        let s = match pick {
            CanvasPick::Block(id) => Some(Selected::Block(id)),
            CanvasPick::Module(m, ids) => Some(Selected::Module(m, ids)),
            CanvasPick::Clear => None,
        };
        if let (Some(s), Some(BrowserFocus(mut f))) = (s.as_ref(), focus)
            && let Some(at) = focus_for(s, &blocks, &comp.peek())
        {
            f.set(Some(at));
        }
        let mut sel = sel;
        sel.set(s);
    };
    rsx! {
        div { style: "position: relative; height: 100%; min-height: 0; background: {DESK}; overflow: hidden;",
            super::routing_canvas::RoutingCanvas { modules, selected, fold: false, fit: 0, focus: ("Input".to_string(), "Master".to_string()), on_pick }
        }
    }
}

/// A module's blocks in up to three rows, then on to the next column: the
/// chain runs down each column and left to right.
const MODULE_ROWS: usize = 4;

/// The Core's own blocks: tagged, wherever they sit.
const CORE_BLOCKS: [&str; 4] = ["Pre Comp", "Gate", "Post Comp", "Amp EQ"];
/// The amps' EQ: in the Amp module, under the amps.
const AMP_EQ: &str = "Amp EQ";

/// The playing blocks as the canvas draws them: runs of one module, in
/// chain order. The pre effects run in a line; delays and reverbs sit
/// either side of the dry; the Amp is its two amps into two cabs, then
/// what shapes it. The Core is a tag on what it owns, not a box.
pub(super) fn canvas_modules(blocks: &[LiveBlock], faces: &crate::rig_faces::Faces) -> Vec<CanvasModule> {
    let mut runs: Vec<(String, Vec<CanvasCell>)> = Vec::new();
    // Everything ahead of the drive board is the Input column.
    let mut past_input = false;
    // The amps show loaded or not; a cab is part of its amp, and the
    // patch's trim is a level, not a block to edit here.
    for b in blocks.iter().filter(|b| (!b.empty || b.module == "Amp" || b.module == "Drive") && b.block_type != BlockType::Cabinet) {
        let is = |names: &[&str]| names.iter().any(|n| n.eq_ignore_ascii_case(&b.name));
        // The Core's blocks: the amp EQ with the amps, the rest (the
        // compressors and the gate) dynamics of their own.
        past_input |= b.module == "Drive";
        let module = if b.name.eq_ignore_ascii_case("Master EQ") || b.name.eq_ignore_ascii_case("Limiter") {
            // The master: the patch's last EQ and the limiter.
            "Master".to_string()
        } else if !past_input {
            // What the guitar meets first, in its four slots (see
            // `input_items`), no module's.
            "Input".to_string()
        } else if matches!(b.module.as_str(), "Delay" | "Reverb") {
            // The delays and the reverbs: one Time column.
            "Time".to_string()
        } else if b.name.eq_ignore_ascii_case(AMP_EQ) {
            "Amp".to_string()
        } else if b.module == "Core" || (b.name.eq_ignore_ascii_case("Boost") && b.module != "Drive") || b.name.eq_ignore_ascii_case("Patch Trim") {
            // The post-amp boost and the patch's trim (what levels the
            // patches) sit with the dynamics.
            "Dynamics".to_string()
        } else {
            b.module.clone()
        };
        let cell = CanvasCell {
            id: b.id.clone(),
            name: if b.module == "Pre" { pre_name(b) } else { b.name.clone() },
            sub: if b.preset.is_empty() || b.preset == b.name { b.detail.clone() } else { b.preset.clone() },
            kind: b.block_type.as_str().to_lowercase(),
            colour: if b.module == "Motion" { MOTION } else { type_colour(b.block_type) }.to_string(),
            lit: !b.bypassed,
            edited: b.overridden,
            empty: b.empty,
            core: is(&CORE_BLOCKS),
            // Its block face, else its unit's own (a drive's pedal).
            keys: face_keys(b),
            fallback: pedal(b, faces),
            value: level_of(b),
            // The trim levels the patch: under the dynamics, not theirs.
            loose: b.name.eq_ignore_ascii_case("Patch Trim"),
            params: b.params.iter().map(|p| (p.name.clone(), f64::from(p.value))).chain([("on".to_string(), if b.bypassed { 0.0 } else { 1.0 })]).collect(),
        };
        // The amp EQ joins the amps it follows, though the gate and the
        // post compressor sit between them in the chain: the amps into
        // their EQ, then the dynamics.
        // So does the patch's trim with the dynamics, though the modulation
        // sits between them.
        let levels = b.name.eq_ignore_ascii_case("Patch Trim") || b.name.eq_ignore_ascii_case("Boost");
        if (module == "Amp" || (module == "Dynamics" && levels))
            && let Some((_, cells)) = runs.iter_mut().rev().find(|(m, _)| *m == module)
        {
            cells.push(cell);
            continue;
        }
        match runs.last_mut() {
            Some((m, cells)) if *m == module => cells.push(cell),
            _ => runs.push((module, vec![cell])),
        }
    }
    runs.into_iter()
        .map(|(name, cells)| {
            let items = match name.as_str() {
                // The pre effects down one column: the grid is four rows.
                "Pre" => cells.chunks(4).map(|c| CanvasItem::Col(c.to_vec())).collect(),
                // The delays and reverbs side by side with the dry, one column.
                "Time" => cells.chunks(4).map(|c| CanvasItem::Lanes(c.iter().cloned().zip(-1..).map(|(c, l)| (l, c)).collect())).collect(),
                "Amp" => amp_items(cells),
                "Input" => input_items(cells),
                _ => cells.chunks(MODULE_ROWS).map(|c| CanvasItem::Col(c.to_vec())).collect(),
            };
            let name = if name == "Pre" { "Pre-FX".to_string() } else { name };
            CanvasModule {
                colour: module_colour(&name).to_string(),
                label: String::new(),
                items,
                core: matches!(name.as_str(), "Drive" | "Amp"),
                bare: name == "Input",
                name,
            }
        })
        .collect()
}

/// A pre effect by what it is, not its slot: its delay "Delay", its reverb
/// "Verb", and its mod and motion slots the effect they hold (a chorus,
/// a phaser, a tremolo…).
fn pre_name(b: &LiveBlock) -> String {
    match b.block_type {
        BlockType::Delay => "Delay".into(),
        BlockType::Reverb => "Verb".into(),
        BlockType::Trem => "Tremolo".into(),
        t => {
            let n = t.as_str();
            let mut c = n.chars();
            c.next().map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
        }
    }
}

/// The unit a block is, as its face and namespace: a drive slot's or a
/// pre effect's pedal, an amp's own amp (its block composition).
fn pedal(b: &LiveBlock, faces: &crate::rig_faces::Faces) -> Option<(String, String, bool)> {
    if b.empty {
        return None;
    }
    let stem = |f: &crate::rig_faces::FaceEntry| f.face.trim_start_matches("rig-faces/").to_string();
    let name = b.name.to_lowercase();
    match b.module.as_str() {
        // The input stage's units: the dive bomb, the transposer, the volume
        // pedal (the octaver, the filter wear their pictures).
        _ if ["dive", "transpose", "volume pedal"].iter().any(|w| name.contains(w)) => {
            let f = faces.input.iter().find(|e| e.words.iter().any(|w| name.contains(w.as_str())))?;
            Some((stem(f), f.ns.clone(), false))
        }
        "Drive" | "Pre" => super::fx_row::face_for(b, faces).map(|f| (stem(&f), f.ns, false)),
        "Amp" if b.block_type == BlockType::Amp => {
            let f = faces.amp(&b.asset).or_else(|| faces.amp(&b.preset))?;
            Some((format!("{}-block", stem(f)), f.ns.clone(), true))
        }
        _ => None,
    }
}

/// A level block's level, as the block shows it: the post-amp boost and
/// the patch's trim, in dB.
fn level_of(b: &LiveBlock) -> Option<String> {
    if b.block_type != BlockType::Volume || b.name.eq_ignore_ascii_case("Volume Ped") {
        return None;
    }
    let db = b.params.iter().find(|p| p.name == "gain_db")?.value;
    Some(format!("{}{db:.1} dB", if db > 0.0 { "+" } else { "" }))
}

/// What picks a block's frame face, most particular first: a delay by its
/// machine, a reverb by its algorithm, then its type.
fn face_keys(b: &LiveBlock) -> Vec<String> {
    let kind = b.block_type.as_str().to_lowercase();
    let param = |n: &str| b.params.iter().find(|p| p.name == n).map(|p| p.value.round().max(0.0) as usize);
    let named = match b.block_type {
        BlockType::Delay => param("style").and_then(|i| crate::control::DELAY_ALGOS.get(i)),
        BlockType::Reverb => param("algorithm").and_then(|i| crate::control::VERB_ALGOS.get(i)),
        _ => None,
    };
    let mut keys: Vec<String> = named.map(|n| format!("{kind}:{}", n.to_lowercase())).into_iter().collect();
    match b.module.as_str() {
        // A drive slot by the pedal it plays (`drive:klon centaur`).
        "Drive" if !b.preset.is_empty() => keys.insert(0, format!("drive:{}", b.preset.to_lowercase())),
        // A pre effect by its kind first (`pre:chorus`).
        "Pre" => keys.insert(0, format!("pre:{kind}")),
        // The post compressor by its unit, the Distressor.
        _ if b.name.eq_ignore_ascii_case("Post Comp") => keys.insert(0, "post comp".to_string()),
        _ => {}
    }
    keys.push(kind);
    keys
}

/// The Input column's four slots: a compressor; a pitch effect (the
/// octaver, the harmonizer); an envelope filter or a wah; the volume pedal
/// when the patch plays it ahead of the drives, else the dive bomb. Each
/// the patch's block for it that is on (else off), or an empty slot — the
/// filter's wearing the Q-Tron, the last a Special for what comes later.
fn input_items(cells: Vec<CanvasCell>) -> Vec<CanvasItem> {
    let mut left = cells;
    let mut slot = |names: &[&str], kinds: &[&str], lit_only: bool, empty: (&str, &str, &[&str])| {
        let of = |c: &CanvasCell| names.iter().any(|n| c.name.eq_ignore_ascii_case(n)) || kinds.contains(&c.kind.as_str());
        // The first named that is on (in the names' order), else any of
        // its kinds on, else (unless only on will do) any off.
        let named = names.iter().find_map(|n| left.iter().position(|c| c.lit && c.name.eq_ignore_ascii_case(n)));
        let at = named.or_else(|| left.iter().position(|c| of(c) && c.lit)).or_else(|| (!lit_only).then(|| left.iter().position(of)).flatten());
        match at {
            Some(i) => left.remove(i),
            None => CanvasCell {
                id: format!("slot:{}", empty.0),
                name: empty.0.to_string(),
                sub: String::new(),
                kind: empty.1.to_string(),
                colour: GREY.to_string(),
                lit: false,
                edited: false,
                empty: empty.2.is_empty(),
                core: false,
                keys: empty.2.iter().map(|k| (*k).to_string()).collect(),
                params: vec![("on".to_string(), 0.0)],
                value: None,
                loose: false,
                fallback: None,
            },
        }
    };
    vec![CanvasItem::Col(vec![
        slot(&["Pre Comp"], &["compressor"], false, ("Comp", "compressor", &[])),
        slot(&["Pitch", "Harmonizer"], &["doubler"], false, ("Pitch", "pitch", &["pitch"])),
        slot(&[], &["filter", "wah"], false, ("Filter", "filter", &["filter"])),
        slot(&["Volume Pedal", "Dive Bomb"], &[], true, ("Special", "special", &[])),
    ])]
}


const GREY: &str = "#a1a1aa";
/// Motion (tremolo, vibrato, rotary): green, apart from the cool modulation
/// and the violet reverbs.
const MOTION: &str = "#34d399";

/// The Amp module: one column — Amp L and Amp R (each with its cab) on
/// the top two rows, into the amp EQ on the third.
fn amp_items(cells: Vec<CanvasCell>) -> Vec<CanvasItem> {
    let find = |n: &str| cells.iter().find(|c| c.name.eq_ignore_ascii_case(n)).cloned();
    let (Some(al), Some(ar)) = (find("Amp L"), find("Amp R")) else {
        return cells.chunks(MODULE_ROWS).map(|c| CanvasItem::Col(c.to_vec())).collect();
    };
    let amps = vec![(-1, al), (0, ar)];
    let mut items = vec![match find(AMP_EQ) {
        Some(eq) => CanvasItem::Merge(amps, (1, eq)),
        None => CanvasItem::Lanes(amps),
    }];
    let placed = ["Amp L", "Amp R", AMP_EQ];
    let rest: Vec<CanvasCell> = cells.iter().filter(|c| !placed.iter().any(|n| c.name.eq_ignore_ascii_case(n))).cloned().collect();
    items.extend(rest.chunks(MODULE_ROWS).map(|c| CanvasItem::Col(c.to_vec())));
    items
}

/// A module's colour on the grid: drives orange, amps yellow, modulation
/// cyan, delays blue, reverbs violet; everything else grey.
fn module_colour(name: &str) -> &'static str {
    match name {
        "Drive" => "#f97316",
        "Amp" => "#eab308",
        "Modulation" => "#22d3ee",
        "Motion" => MOTION,
        "Delay" => "#3b82f6",
        "Time" => "#6366f1",
        "Reverb" => "#8b5cf6",
        _ => GREY,
    }
}

fn type_colour(t: BlockType) -> &'static str {
    match t {
        BlockType::Drive | BlockType::Boost | BlockType::Saturator => "#f97316",
        BlockType::Amp | BlockType::Cabinet => "#eab308",
        BlockType::Chorus | BlockType::Flanger | BlockType::Phaser => "#22d3ee",
        BlockType::Trem | BlockType::Rotary | BlockType::Vibrato => MOTION,
        BlockType::Delay => "#3b82f6",
        BlockType::Reverb => "#8b5cf6",
        _ => GREY,
    }
}
