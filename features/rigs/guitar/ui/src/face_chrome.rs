//! What a face carries besides itself: its block's preset — named, stepped
//! with ‹ ›, the name opening the block's presets in the browser — and,
//! where the unit hides what it is doing (the post compressor), a switch
//! to its live visualiser.
//!
//! Both read context the Control view provides: [`FacePresets`] (the
//! composition libraries and what plays) and [`VizMode`] (the faces shown
//! as their visualisers).

use std::collections::HashSet;

use dioxus::prelude::*;
use signal_guitar_proto::CompositionModel;

use signal_guitar_proto::rig::RigClient;

/// The composition libraries and what the patch plays from them, for the
/// faces' preset steppers.
#[derive(Clone, Copy)]
pub struct FacePresets(pub Signal<CompositionModel>);

/// The blocks (by name) shown as their visualiser instead of their face.
#[derive(Clone, Copy)]
pub struct VizMode(pub Signal<HashSet<String>>);

/// Whether `block` shows its visualiser.
#[must_use]
pub fn shows_viz(viz: Option<VizMode>, block: &str) -> bool {
    viz.is_some_and(|v| v.0.read().contains(block))
}

/// A block's preset: ‹ name ›. The arrows step through its type's presets
/// (wrapping); the name opens them in the browser.
#[component]
pub fn PresetStepper(
    /// The block's name (`DLY 1`).
    block: String,
    /// Its type as the libraries file it (`delay`, `reverb`, `chorus`).
    block_type: String,
    /// The name's colour.
    #[props(default = "#e4e4e7".to_string())]
    accent: String,
    /// Show even with no presets of its type yet (the name still opens
    /// the browser, to save the first) — a separator's section; a face's
    /// corner hides it instead.
    #[props(default)]
    show_empty: bool,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let select = try_use_context::<crate::module_sidebar::SelectedModule>();
    let comp = try_use_context::<FacePresets>().map(|c| c.0.read().clone()).unwrap_or_default();
    let names: Vec<String> = comp
        .block_presets
        .iter()
        .filter(|p| p.block_type.eq_ignore_ascii_case(&block_type))
        .map(|p| p.name.clone())
        .collect();
    let playing = comp.active_blocks.iter().find(|b| b.block.eq_ignore_ascii_case(&block)).map(|b| b.preset.clone()).unwrap_or_default();
    let at = names.iter().position(|n| n.eq_ignore_ascii_case(&playing));
    let label = if !playing.is_empty() {
        playing.clone()
    } else if names.is_empty() {
        "No presets yet".to_string()
    } else {
        "—".to_string()
    };
    let step = {
        let (rig, block, names) = (rig.clone(), block.clone(), names.clone());
        move |dir: i32| {
            if names.is_empty() {
                return;
            }
            let n = names.len() as i32;
            let next = at.map_or(0, |i| (((i as i32 + dir) % n) + n) % n) as usize;
            let (rig, block, name) = (rig.clone(), block.clone(), names[next].clone());
            spawn(async move {
                if let Some(r) = rig {
                    let _ = r.choose_block(block, name).await;
                }
            });
        }
    };
    // Nothing to step through (no presets of its type): no stepper.
    if names.is_empty() && !show_empty {
        return rsx! {};
    }
    let arrow = "font-size: 11px; line-height: 1; color: #a1a1aa; padding: 0 3px; cursor: pointer;";
    rsx! {
        div { style: "display: flex; align-items: center; gap: 1px; min-width: 0;",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            span { style: "{arrow}", onclick: { let step = step.clone(); move |_| step(-1) }, "‹" }
            span {
                style: "font-size: 10px; font-weight: 600; color: {accent}; max-width: 118px; overflow: hidden; white-space: nowrap; cursor: pointer; padding: 1px 4px; border-radius: 3px; background: rgba(255,255,255,0.06);",
                title: "Browse {block}'s presets",
                onclick: {
                    let (block, block_type) = (block.clone(), block_type.clone());
                    move |_| {
                        if let Some(s) = select {
                            s.set(crate::module_sidebar::Selection::Block { name: block.clone(), block_type: block_type.clone() });
                        }
                    }
                },
                "{label}"
            }
            span { style: "{arrow}", onclick: move |_| step(1), "›" }
        }
    }
}

/// Face ↔ visualiser, for `block`: a small switch, lit while the
/// visualiser shows.
#[component]
pub fn VizToggle(block: String) -> Element {
    let Some(mode) = try_use_context::<VizMode>() else {
        return rsx! {};
    };
    let on = mode.0.read().contains(&block);
    let (bg, fg) = if on { ("rgba(125,211,252,0.22)", "#7dd3fc") } else { ("rgba(255,255,255,0.06)", "#a1a1aa") };
    rsx! {
        span {
            style: "font-size: 9px; font-weight: 700; letter-spacing: 0.06em; line-height: 1; color: {fg}; background: {bg}; padding: 3px 5px; border-radius: 3px; cursor: pointer;",
            title: if on { "Show the unit" } else { "Show what it's doing" },
            onclick: move |e: MouseEvent| {
                e.stop_propagation();
                let mut set = mode.0;
                let mut s = set.write();
                if !s.remove(&block) {
                    s.insert(block.clone());
                }
            },
            if on { "UNIT" } else { "VIZ" }
        }
    }
}
