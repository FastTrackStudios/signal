//! Routing — the patch playing as a grid: its blocks as cells in chain
//! order, each module (Core, Amp, Drive, Time…) a field behind its cells
//! with the preset · variation it plays, cables between them. A stereo pair
//! (Amp L / Amp R) stacks in one column, so a row is as tall as the patch
//! needs.
//!
//! Two layouts: **fold** wraps the chain at the view's width — a long module
//! goes on down to the next row, the cable bending back to it — and
//! **unfolded** is one row, scrolled sideways, sized to the view's height.
//! Pinch zooms either; one finger scrolls.
//!
//! A tap selects a block or a whole module: the FX row below shows its
//! controls, and the browser beside it opens on it — its kind, and the
//! preset it plays.

use std::collections::BTreeMap;
use std::rc::Rc;

use dioxus::prelude::*;
use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{CompositionModel, LiveBlock};

use super::marks::{block_colour, module_colour, ModuleIcon};
use super::tokens::*;
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

// The grid's measures, pt at zoom 1.
const TILE_W: f64 = 124.0;
const TILE_H: f64 = 78.0;
const GAP: f64 = 8.0;
const PAD: f64 = 8.0;
const HEAD: f64 = 42.0;
const CABLE: f64 = 30.0;
const ROW_GAP: f64 = 34.0;
const EDGE: f64 = 20.0;
const END_W: f64 = 48.0;

/// A module's run of the chain: its columns (a block, or a stereo pair).
#[derive(Clone, PartialEq)]
struct Module {
    name: String,
    preset: String,
    snapshot: String,
    cols: Vec<Vec<LiveBlock>>,
}

/// The chain as modules: consecutive blocks of one module (each block says
/// its own), an `… L` / `… R` pair in one column.
fn modules(blocks: &[LiveBlock], comp: &CompositionModel) -> Vec<Module> {
    let mut out: Vec<Module> = Vec::new();
    for b in blocks {
        let new = out.last().is_none_or(|m| m.name != b.module);
        if new {
            let pick = comp.active_modules.iter().find(|m| m.module.eq_ignore_ascii_case(&b.module));
            out.push(Module {
                name: b.module.clone(),
                preset: pick.map(|m| m.preset.clone()).unwrap_or_default(),
                snapshot: pick.map(|m| m.snapshot.clone()).unwrap_or_default(),
                cols: Vec::new(),
            });
        }
        let m = out.last_mut().expect("pushed");
        // The right of a stereo pair joins its left.
        let pair = b.name.strip_suffix(" R").and_then(|stem| {
            m.cols.iter().position(|c| c.len() == 1 && c[0].name.strip_suffix(" L") == Some(stem))
        });
        match pair {
            Some(i) => m.cols[i].push(b.clone()),
            None => m.cols.push(vec![b.clone()]),
        }
    }
    out
}

/// One piece of a module on the grid (a long one folds into several).
#[derive(Clone, PartialEq)]
struct Piece {
    module: usize,
    cols: std::ops::Range<usize>,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    /// The first piece carries the module's header.
    first: bool,
}

fn lanes(m: &Module) -> usize {
    m.cols.iter().map(Vec::len).max().unwrap_or(1).max(1)
}

fn piece_w(n: usize) -> f64 {
    PAD * 2.0 + n as f64 * TILE_W + n.saturating_sub(1) as f64 * GAP
}

fn piece_h(lanes: usize) -> f64 {
    HEAD + lanes as f64 * TILE_H + lanes.saturating_sub(1) as f64 * GAP + PAD
}

/// Lay the modules out: in rows `width` wide (fold), or in one row.
/// Returns the pieces and the content's size, at zoom 1.
fn layout(mods: &[Module], width: Option<f64>) -> (Vec<Piece>, f64, f64) {
    let mut pieces: Vec<Piece> = Vec::new();
    let start = EDGE + END_W + CABLE;
    let (mut x, mut row_top, mut row_h) = (start, EDGE, 0.0_f64);
    let mut max_x = x;
    for (mi, m) in mods.iter().enumerate() {
        let h = piece_h(lanes(m));
        let mut c = 0;
        while c < m.cols.len() {
            // How many columns fit what is left of the row.
            let room = width.map_or(f64::INFINITY, |w| w - EDGE - x);
            let mut n = m.cols.len() - c;
            if piece_w(n) > room {
                n = (((room - PAD * 2.0 + GAP) / (TILE_W + GAP)).floor().max(0.0)) as usize;
                if n == 0 {
                    // Nothing fits here: on to the next row.
                    if x > start {
                        row_top += row_h + ROW_GAP;
                        row_h = 0.0;
                        x = start;
                        continue;
                    }
                    n = 1;
                }
                // A module that would fit whole on a fresh row goes there
                // rather than splitting.
                if c == 0 && x > start && width.is_some_and(|w| piece_w(m.cols.len()) <= w - EDGE - start) {
                    row_top += row_h + ROW_GAP;
                    row_h = 0.0;
                    x = start;
                    continue;
                }
            }
            let w = piece_w(n);
            pieces.push(Piece { module: mi, cols: c..c + n, x, y: row_top, w, h, first: c == 0 });
            row_h = row_h.max(h);
            x += w + CABLE;
            max_x = max_x.max(x);
            c += n;
        }
    }
    let total_h = row_top + row_h + EDGE;
    (pieces, max_x + END_W + EDGE, total_h)
}

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
    use_effect(move || {
        let _ = state.blocks.read();
        let _ = state.active_patch.read();
        if let Some(r) = rig.clone() {
            spawn(async move {
                if let Ok(c) = r.compositions().await {
                    comp.set(c);
                }
            });
        }
    });
    // The view's size, measured once laid out.
    let mut view = use_signal(|| (0.0_f64, 0.0_f64));
    let mut view_el = use_signal(|| None::<Rc<MountedData>>);
    let measure = move || {
        let Some(el) = view_el.peek().clone() else { return };
        spawn(async move {
            for _ in 0..10 {
                if let Ok(r) = el.get_client_rect().await
                    && r.width() > 1.0
                {
                    let now = (r.width(), r.height());
                    if *view.peek() != now {
                        view.set(now);
                    }
                    return;
                }
                architect::platform::sleep(std::time::Duration::from_millis(50)).await;
            }
        });
    };
    // Measured again as the patch changes (Blitz has no resize event).
    use_effect(move || {
        let _ = state.blocks.read();
        measure();
    });
    // Fold, and the zoom (None: fitted — unfolded, to the view's height).
    let mut fold = use_signal(|| true);
    let mut zoom = use_signal(|| None::<f64>);
    // Fingers down, for the pinch: id → position; and the pinch's start.
    let mut fingers = use_signal(BTreeMap::<i32, (f64, f64)>::new);
    let mut pinch = use_signal(|| None::<(f64, f64)>);

    let blocks = state.blocks.read().clone();
    let c = comp.read().clone();
    let mods = modules(&blocks, &c);
    let (vw, vh) = view();
    // Unfolded and not pinched: the row fitted to the view's height.
    let natural_h = layout(&mods, None).2;
    let fitted = if vh > 1.0 { (vh / natural_h).clamp(0.6, 1.6) } else { 1.0 };
    let z = zoom().unwrap_or(if fold() { 1.0 } else { fitted });
    let wrap = (fold() && vw > 1.0).then(|| vw / z);
    let (pieces, cw, ch) = layout(&mods, wrap);
    let selected = sel();
    let choose = move |s: Selected| {
        let blocks = state.blocks.peek().clone();
        if let Some(BrowserFocus(mut f)) = focus
            && let Some(at) = focus_for(&s, &blocks, &comp.peek())
        {
            f.set(Some(at));
        }
        let mut sel = sel;
        sel.set(Some(s));
    };
    let lane1 = HEAD + TILE_H / 2.0;
    let cables = cable_path(&pieces, cw);
    rsx! {
        div { style: "position: relative; height: 100%; min-height: 0; background: {DESK};",
            div {
                style: "position: absolute; left: 0; right: 0; top: 0; bottom: 0; overflow: auto;",
                onmounted: move |e| {
                    view_el.set(Some(e.data()));
                    measure();
                },
                // Two fingers pinch; one scrolls.
                onpointerdown: move |e: PointerEvent| {
                    let p = e.client_coordinates();
                    fingers.write().insert(e.pointer_id(), (p.x, p.y));
                    if fingers.peek().len() == 2 {
                        pinch.set(Some((spread(&fingers.peek()), z)));
                    }
                },
                onpointermove: move |e: PointerEvent| {
                    let id = e.pointer_id();
                    if !fingers.peek().contains_key(&id) {
                        return;
                    }
                    let p = e.client_coordinates();
                    fingers.write().insert(id, (p.x, p.y));
                    if let Some((d0, z0)) = pinch()
                        && fingers.peek().len() == 2
                        && d0 > 1.0
                    {
                        e.prevent_default();
                        let next = (z0 * spread(&fingers.peek()) / d0).clamp(0.5, 2.0);
                        zoom.set(Some(next));
                    }
                },
                onpointerup: move |e: PointerEvent| {
                    fingers.write().remove(&e.pointer_id());
                    if fingers.peek().len() < 2 {
                        pinch.set(None);
                    }
                },
                onpointercancel: move |e: PointerEvent| {
                    fingers.write().remove(&e.pointer_id());
                    pinch.set(None);
                },
                // The content, at the zoom: everything sized, nothing scaled
                // (a scaled box is hit where it was laid out).
                div { style: "position: relative; width: {cw * z}px; height: {ch * z}px;",
                    svg { width: "{cw * z}", height: "{ch * z}", view_box: "0 0 {cw} {ch}", style: "position: absolute; left: 0; top: 0;",
                        path { d: "{cables}", fill: "none", stroke: "#3f3f46", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round" }
                    }
                    End { label: "IN", x: EDGE * z, y: (EDGE + lane1) * z, z }
                    if let Some(last) = pieces.last() {
                        End { label: "OUT", x: (last.x + last.w + CABLE) * z, y: (last.y + lane1) * z, z }
                    }
                    for (i, p) in pieces.iter().cloned().enumerate() {
                        ModulePiece {
                            key: "{i}",
                            module: mods[p.module].clone(),
                            piece: p.clone(),
                            z,
                            selected: selected.clone(),
                            on_pick: move |s: Selected| choose(s),
                        }
                    }
                }
            }
            // The layout's switch and the zoom back to fit.
            div { style: "position: absolute; top: 8px; right: 10px; display: flex; gap: 6px; z-index: 2;",
                if zoom().is_some() {
                    Chip { label: "Fit", on: false, onclick: move |_| zoom.set(None) }
                }
                Chip { label: "Fold", on: fold(), onclick: move |_| {
                    fold.toggle();
                    zoom.set(None);
                } }
            }
        }
    }
}

/// The distance between the two fingers down.
fn spread(f: &BTreeMap<i32, (f64, f64)>) -> f64 {
    let mut it = f.values();
    match (it.next(), it.next()) {
        (Some(a), Some(b)) => ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt(),
        _ => 0.0,
    }
}

/// The cables, at zoom 1: from IN to the first piece, piece to piece along
/// a row, bending back down to the next row's start, and on to OUT.
fn cable_path(pieces: &[Piece], width: f64) -> String {
    let lane = HEAD + TILE_H / 2.0;
    let mut d = String::new();
    let Some(first) = pieces.first() else { return d };
    d.push_str(&format!("M{} {} H{} ", EDGE + END_W, first.y + lane, first.x));
    for w in pieces.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        let (ax, ay, bx, by) = (a.x + a.w, a.y + lane, b.x, b.y + lane);
        if (ay - by).abs() < 0.5 {
            d.push_str(&format!("M{ax} {ay} H{bx} "));
        } else {
            // Down to the gap under the row, back to the left, down to the
            // next piece.
            let mid = a.y + row_bottom(pieces, a.y) + ROW_GAP / 2.0;
            let back = (b.x - CABLE / 2.0).max(EDGE);
            let out = (ax + CABLE / 2.0).min(width - EDGE / 2.0);
            d.push_str(&format!("M{ax} {ay} H{out} V{mid} H{back} V{by} H{bx} "));
        }
    }
    if let Some(last) = pieces.last() {
        d.push_str(&format!("M{} {} H{} ", last.x + last.w, last.y + lane, last.x + last.w + CABLE));
    }
    d
}

/// The bottom of the row starting at `top`, relative to it.
fn row_bottom(pieces: &[Piece], top: f64) -> f64 {
    pieces.iter().filter(|p| (p.y - top).abs() < 0.5).map(|p| p.h).fold(0.0, f64::max)
}

/// The grid's ends: where the guitar comes in, and where it goes out.
#[component]
fn End(label: &'static str, x: f64, y: f64, z: f64) -> Element {
    let (w, h) = (END_W * z, 30.0 * z);
    rsx! {
        div { style: "position: absolute; left: {x}px; top: {y - h / 2.0}px; width: {w}px; height: {h}px; display: flex; align-items: center; justify-content: center; border-radius: 999px; border: 1.5px solid {RULE_STRONG}; background: {DESK}; box-sizing: border-box; font-size: {11.0 * z}px; font-weight: 800; letter-spacing: 0.1em; color: {INK_3};",
            "{label}"
        }
    }
}

/// A small toggle over the grid.
#[component]
fn Chip(label: &'static str, on: bool, onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            style: "height: 32px; padding: 0 12px; border-radius: 999px; border: 1px solid {pick(on, INK_2, RULE_STRONG)}; background: {pick(on, FILL_ON, SHEET)}; color: {pick(on, INK, INK_2)}; font-family: {FONT}; font-size: 12.5px; font-weight: 700; box-sizing: border-box; cursor: pointer;",
            onclick: move |e| onclick.call(e),
            "{label}"
        }
    }
}

/// A module's field (or a folded piece of it) with its cells. The header
/// selects the module; a cell selects its block.
#[component]
fn ModulePiece(module: Module, piece: Piece, z: f64, selected: Option<Selected>, on_pick: EventHandler<Selected>) -> Element {
    let named = ["Core", "Amp", "Drive", "Time", "Delay", "Reverb"].contains(&module.name.as_str());
    let colour = if named { module_colour(&module.name).to_string() } else { INK_3.to_string() };
    let ids: Vec<String> = module.cols.iter().flatten().map(|b| b.id.clone()).collect();
    let module_on = matches!(&selected, Some(Selected::Module(m, i)) if *m == module.name && *i == ids);
    let preset = match (module.preset.is_empty(), module.snapshot.is_empty()) {
        (true, _) => String::new(),
        (false, true) => module.preset.clone(),
        (false, false) => format!("{} · {}", module.preset, module.snapshot),
    };
    let edge = if module_on { colour.clone() } else { RULE.to_string() };
    let bg = if module_on { tint(&colour, 12) } else { tint(&colour, 5) };
    let pick_module = {
        let (m, ids) = (module.name.clone(), ids.clone());
        move |_| on_pick.call(Selected::Module(m.clone(), ids.clone()))
    };
    rsx! {
        div { style: "position: absolute; left: {piece.x * z}px; top: {piece.y * z}px; width: {piece.w * z}px; height: {piece.h * z}px; border-radius: {10.0 * z}px; border: 1.5px solid {edge}; background: {bg}; box-sizing: border-box;",
            // The module: its colour along the top, its name, what it plays.
            button {
                style: "position: absolute; left: 0; right: 0; top: 0; height: {HEAD * z}px; display: flex; align-items: center; justify-content: flex-start; gap: {7.0 * z}px; padding: 0 {10.0 * z}px; border: none; background: transparent; text-align: left; font-family: {FONT}; cursor: pointer; box-sizing: border-box; overflow: hidden;",
                onclick: pick_module,
                span { style: "position: absolute; left: 0; right: 0; top: 0; height: 3px; border-radius: 10px 10px 0 0; background: {colour};" }
                if named {
                    ModuleIcon { kind: module.name.clone(), size: (14.0 * z) as u32, colour: colour.clone() }
                }
                span { style: "font-size: {11.0 * z}px; font-weight: 800; letter-spacing: 0.08em; text-transform: uppercase; color: {lift(&colour)}; white-space: nowrap;",
                    if piece.first { "{module.name}" } else { "{module.name} ›" }
                }
                if piece.first && !preset.is_empty() {
                    span { style: "font-size: {12.0 * z}px; font-weight: 600; color: {INK_2}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0;", "{preset}" }
                }
            }
            for (k, col) in module.cols[piece.cols.clone()].iter().enumerate() {
                for (lane, b) in col.iter().enumerate() {
                    Cell {
                        key: "{b.id}",
                        block: b.clone(),
                        x: (PAD + k as f64 * (TILE_W + GAP)) * z,
                        y: (HEAD + lane as f64 * (TILE_H + GAP)) * z,
                        z,
                        on: matches!(&selected, Some(Selected::Block(id)) if *id == b.id),
                        on_pick: move |s: Selected| on_pick.call(s),
                    }
                }
            }
        }
    }
}

/// One block's cell: its type's colour, its name, the preset it plays, and
/// its light (on or off — a tap on it switches). A tap elsewhere selects it.
#[component]
fn Cell(block: LiveBlock, x: f64, y: f64, z: f64, on: bool, on_pick: EventHandler<Selected>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let t = block.block_type.as_str().to_lowercase();
    let colour = block_colour(&t).to_string();
    let lit = !block.bypassed && !block.empty;
    let sub = if block.detail.is_empty() { block.preset.clone() } else { format!("{} · {}", block.preset, block.detail) };
    let edge = if on { INK.to_string() } else { RULE.to_string() };
    let bg = if on { tint(&colour, 24) } else if lit { tint(&colour, 12) } else { FILL.to_string() };
    let ink = if lit { INK } else { INK_3 };
    let light = if lit { LIVE } else { DIM };
    let id = block.id.clone();
    rsx! {
        div { style: "position: absolute; left: {x}px; top: {y}px; width: {TILE_W * z}px; height: {TILE_H * z}px; border-radius: {7.0 * z}px; border: {pick(on, 2.0, 1.0)}px solid {edge}; background: {bg}; box-sizing: border-box; overflow: hidden;",
            button {
                style: "position: absolute; left: 0; top: 0; right: 0; bottom: 0; display: flex; flex-direction: column; align-items: flex-start; justify-content: flex-start; gap: {3.0 * z}px; padding: {8.0 * z}px {26.0 * z}px {6.0 * z}px {10.0 * z}px; border: none; background: transparent; text-align: left; font-family: {FONT}; cursor: pointer; box-sizing: border-box;",
                onclick: {
                    let id = id.clone();
                    move |_| on_pick.call(Selected::Block(id.clone()))
                },
                span { style: "position: absolute; left: 0; top: {8.0 * z}px; bottom: {8.0 * z}px; width: 3px; border-radius: 0 2px 2px 0; background: {colour}; opacity: {pick(lit, 1.0, 0.4)};" }
                span { style: "font-size: {13.0 * z}px; font-weight: 700; color: {ink}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 100%;", "{block.name}" }
                if !sub.is_empty() {
                    span { style: "font-size: {11.0 * z}px; font-weight: 550; color: {INK_3}; line-height: 1.25; overflow: hidden; max-height: {28.0 * z}px;", "{sub}" }
                }
                if block.overridden {
                    span { style: "margin-top: auto; font-size: {10.5 * z}px; font-weight: 750; color: {MODIFIED};", "Edited" }
                }
            }
            // Its light: on or off.
            button {
                "aria-label": if lit { "Turn {block.name} off" } else { "Turn {block.name} on" },
                style: "position: absolute; top: 0; right: 0; width: {34.0 * z}px; height: {34.0 * z}px; display: flex; align-items: center; justify-content: center; border: none; background: transparent; cursor: pointer;",
                onclick: move |e: MouseEvent| {
                    e.stop_propagation();
                    let id = id.clone();
                    if let Some(r) = rig.clone() {
                        let _ = dioxus_core::spawn_forever(async move { let _ = r.toggle_block_bypass(id).await; });
                    }
                },
                span { style: "width: {9.0 * z}px; height: {9.0 * z}px; border-radius: 999px; background: {light};" }
            }
        }
    }
}
