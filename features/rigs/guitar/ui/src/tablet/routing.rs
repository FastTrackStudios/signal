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
        })
    };
    match sel {
        Selected::Module(m, _) => module_focus(m),
        Selected::Block(id) => {
            let b = blocks.iter().find(|b| b.id == *id)?;
            let t = b.block_type.as_str().to_lowercase();
            if comp.block_presets.iter().any(|p| p.block_type.eq_ignore_ascii_case(&t)) {
                return Some(Focus { kind: format!("block:{t}"), preset: b.preset.clone(), variation: String::new() });
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
    let mut fit = use_signal(|| 0u32);
    let modules = canvas_modules(&blocks);
    let selected = sel().map(|s| match s {
        Selected::Block(id) => CanvasSel::Block(id),
        Selected::Module(m, _) => CanvasSel::Module(m),
    });
    let on_pick = move |pick: CanvasPick| {
        let blocks = state.blocks.peek().clone();
        let s = match pick {
            CanvasPick::Block(id) => Some(Selected::Block(id)),
            CanvasPick::Module(m, ids) => Some(Selected::Module(m, ids)),
            CanvasPick::Toggle(id) => {
                if let (Some(r), Some(b)) = (rig.clone(), blocks.iter().find(|b| b.id == id)) {
                    let on = b.bypassed;
                    let _ = dioxus_core::spawn_forever(async move {
                        let _ = r.set_block_bypass(id, !on).await;
                    });
                }
                return;
            }
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
    let chip = "height: 30px; padding: 0 11px; border-radius: 15px; border: 1px solid #3f3f46; background: rgba(11,11,14,0.86); color: #d4d4d8; font-size: 12.5px; font-weight: 650; display: flex; align-items: center; cursor: pointer;";
    rsx! {
        div { style: "position: relative; height: 100%; min-height: 0; background: {DESK}; overflow: hidden;",
            super::routing_canvas::RoutingCanvas { modules, selected, fold: false, fit: fit(), on_pick }
            div { style: "position: absolute; bottom: 8px; right: 8px; display: flex; gap: 6px;",
                div { style: "{chip}", onclick: move |_| fit += 1, "Fit" }
            }
        }
    }
}

/// A module's blocks in up to three rows, then on to the next column: the
/// chain runs down each column and left to right.
const MODULE_ROWS: usize = 3;

/// The Core's own blocks: tagged, wherever they sit.
const CORE_BLOCKS: [&str; 4] = ["Pre Comp", "Gate", "Post Comp", "Amp EQ"];
/// What the Core plays after the amp: in the Amp module, after the cabs.
const AMP_SHAPING: [&str; 3] = ["Gate", "Post Comp", "Amp EQ"];

/// The playing blocks as the canvas draws them: runs of one module, in
/// chain order. The pre effects run in a line; delays and reverbs sit
/// either side of the dry; the Amp is its two amps into two cabs, then
/// what shapes it. The Core is a tag on what it owns, not a box.
fn canvas_modules(blocks: &[LiveBlock]) -> Vec<CanvasModule> {
    let mut runs: Vec<(String, Vec<CanvasCell>)> = Vec::new();
    // The amp's slots show loaded or not: two amps, two cabs. The patch's
    // trim is a level, not a block to edit here.
    for b in blocks.iter().filter(|b| (!b.empty || b.module == "Amp") && !b.name.eq_ignore_ascii_case("Patch Trim")) {
        let is = |names: &[&str]| names.iter().any(|n| n.eq_ignore_ascii_case(&b.name));
        let module = if is(&AMP_SHAPING) {
            "Amp".to_string()
        } else if b.module == "Core" {
            "Dynamics".to_string()
        } else {
            b.module.clone()
        };
        let cell = CanvasCell {
            id: b.id.clone(),
            name: b.name.clone(),
            sub: if b.preset.is_empty() || b.preset == b.name { b.detail.clone() } else { b.preset.clone() },
            kind: b.block_type.as_str().to_lowercase(),
            colour: if b.module == "Motion" { MOTION } else { type_colour(b.block_type) }.to_string(),
            lit: !b.bypassed,
            edited: b.overridden,
            empty: b.empty,
            core: is(&CORE_BLOCKS),
            keys: face_keys(b),
            params: b.params.iter().map(|p| (p.name.clone(), f64::from(p.value))).chain([("on".to_string(), if b.bypassed { 0.0 } else { 1.0 })]).collect(),
        };
        match runs.last_mut() {
            Some((m, cells)) if *m == module => cells.push(cell),
            _ => runs.push((module, vec![cell])),
        }
    }
    runs.into_iter()
        .map(|(name, cells)| {
            let items = match name.as_str() {
                "Pre" => cells.into_iter().map(|c| CanvasItem::Col(vec![c])).collect(),
                "Delay" | "Reverb" => cells.chunks(2).map(|c| CanvasItem::Split(c.to_vec())).collect(),
                "Amp" => amp_items(cells),
                _ => cells.chunks(MODULE_ROWS).map(|c| CanvasItem::Col(c.to_vec())).collect(),
            };
            let name = if name == "Pre" { "Pre-FX".to_string() } else { name };
            CanvasModule {
                colour: module_colour(&name).to_string(),
                label: String::new(),
                items,
                core: matches!(name.as_str(), "Drive" | "Amp"),
                name,
            }
        })
        .collect()
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
    keys.push(kind);
    keys
}

const GREY: &str = "#a1a1aa";
/// Motion (tremolo, vibrato, rotary): green, apart from the cool modulation
/// and the violet reverbs.
const MOTION: &str = "#34d399";

/// The Amp module: Amp L over Amp R, then Cab L over Cab R — two stereo
/// lanes, each amp into its own cab — then the gate, post compressor and
/// amp EQ.
fn amp_items(cells: Vec<CanvasCell>) -> Vec<CanvasItem> {
    let find = |n: &str| cells.iter().find(|c| c.name.eq_ignore_ascii_case(n)).cloned();
    let (Some(al), Some(ar), Some(cl), Some(cr)) = (find("Amp L"), find("Amp R"), find("Cab L"), find("Cab R")) else {
        return cells.chunks(MODULE_ROWS).map(|c| CanvasItem::Col(c.to_vec())).collect();
    };
    let stage = ["Amp L", "Amp R", "Cab L", "Cab R"];
    let rest: Vec<CanvasCell> = cells.iter().filter(|c| !stage.iter().any(|n| c.name.eq_ignore_ascii_case(n))).cloned().collect();
    let mut items = vec![CanvasItem::Pair(vec![al, ar]), CanvasItem::Pair(vec![cl, cr])];
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
