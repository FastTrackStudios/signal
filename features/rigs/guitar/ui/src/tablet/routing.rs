//! Routing — the patch playing on the rig's routing grid (`grid::RigGraph`,
//! signal-grid-ui's `RigGridPanel`): its modules and blocks as the rig
//! resolves them. A tap selects a block or a whole module: the FX row below
//! shows its controls, and the browser beside it opens on it — its kind,
//! and the preset it plays.

use dioxus::prelude::*;
use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{CompositionModel, LiveBlock};

use super::routing_canvas::{CanvasCell, CanvasModule, CanvasPick, CanvasSel};
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
    let mut fold = use_signal(|| false);
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
    let fold_on = if fold() { "border-color: #a1a1aa; color: #fafafa;" } else { "" };
    let chip = "height: 30px; padding: 0 11px; border-radius: 15px; border: 1px solid #3f3f46; background: rgba(11,11,14,0.86); color: #d4d4d8; font-size: 12.5px; font-weight: 650; display: flex; align-items: center; cursor: pointer;";
    rsx! {
        div { style: "position: relative; height: 100%; min-height: 0; background: {DESK}; overflow: hidden;",
            super::routing_canvas::RoutingCanvas { modules, selected, fold: fold(), fit: fit(), on_pick }
            div { style: "position: absolute; top: 8px; right: 8px; display: flex; gap: 6px;",
                div { style: "{chip} {fold_on}", onclick: move |_| fold.toggle(), "Fold" }
                div { style: "{chip}", onclick: move |_| fit += 1, "Fit" }
            }
        }
    }
}

/// A module's blocks in up to three rows, then on to the next column: the
/// chain runs down each column and left to right.
const MODULE_ROWS: usize = 3;

/// The playing blocks as the canvas draws them: runs of one module, in
/// chain order.
fn canvas_modules(blocks: &[LiveBlock]) -> Vec<CanvasModule> {
    let mut out: Vec<(String, Vec<CanvasCell>)> = Vec::new();
    for b in blocks.iter().filter(|b| !b.empty) {
        let cell = CanvasCell {
            id: b.id.clone(),
            name: b.name.clone(),
            sub: if b.preset.is_empty() || b.preset == b.name { b.detail.clone() } else { b.preset.clone() },
            colour: type_colour(b.block_type).to_string(),
            lit: !b.bypassed,
            edited: b.overridden,
        };
        match out.last_mut() {
            Some((m, cells)) if *m == b.module => cells.push(cell),
            _ => out.push((b.module.clone(), vec![cell])),
        }
    }
    out.into_iter()
        .map(|(name, cells)| CanvasModule {
            colour: super::marks::module_colour(&name).to_string(),
            label: String::new(),
            cols: cells.chunks(MODULE_ROWS).map(<[CanvasCell]>::to_vec).collect(),
            name,
        })
        .collect()
}

fn type_colour(t: BlockType) -> &'static str {
    match t {
        BlockType::Amp => "#f97316",
        BlockType::Drive | BlockType::Boost => "#ef4444",
        BlockType::Cabinet => "#D6B36A",
        t => super::marks::block_colour(&t.as_str().to_lowercase()),
    }
}
