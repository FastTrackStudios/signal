//! The FTS Mic editor.
//!
//! One row of sections per mic (type, pattern, axis, low cut), then the
//! shared controls. Everything is the suite's shared chrome; the only local
//! piece is a captioned dropdown for the long lists (forty mics, nine
//! pattern steps).

use dioxus::prelude::*;
use fts_plug_ui::prelude::*;
use nice_plug::editor::ResizeHint;
use nice_plug::editor::dpi::LogicalSize;
use nice_plug::prelude::*;
use nice_plug_dioxus::{SharedState, use_param_context};

use crate::params::{FILTERS, MICS, MicUiState, PATTERNS, SOLOS, SOURCES};

/// Editor size requested from the host on open.
pub const EDITOR_W: u32 = 980;
pub const EDITOR_H: u32 = 560;
/// Smallest size the surface still works at.
pub const MIN_EDITOR_W: f32 = 760.0;
pub const MIN_EDITOR_H: f32 = 460.0;
/// Largest size the editor accepts (an unbounded hint lets hosts open it
/// absurdly large).
pub const MAX_EDITOR_W: f32 = 1800.0;
pub const MAX_EDITOR_H: f32 = 1200.0;

#[must_use]
pub const fn resize_hint() -> ResizeHint {
    ResizeHint::RESIZABLE.with_min_max_logical_size(
        Some(LogicalSize::new(MIN_EDITOR_W, MIN_EDITOR_H)),
        Some(LogicalSize::new(MAX_EDITOR_W, MAX_EDITOR_H)),
    )
}

#[must_use]
pub const fn skin() -> Skin {
    Skin::accented(accents::MIC)
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_string()).collect()
}

/// A caption over the shared dropdown, which opens downward here: the mic
/// sections sit at the top of the surface.
#[component]
fn Labeled(handle: ParamHandle, testid: String, label: String, options: Vec<String>, skin: Skin, #[props(default = 120.0)] width: f64) -> Element {
    rsx! {
        div {
            style: "display:flex; flex-direction:column; gap:4px;",
            div { style: "font-size:10px; color:{skin.text}; letter-spacing:0.06em;", "{label}" }
            fts_audio_ui::controls::Dropdown {
                handle,
                options,
                color: skin.accent.to_string(),
                width,
                down: true,
                testid: format!("select-{testid}"),
            }
        }
    }
}

/// A caption over a toggle.
#[component]
fn Switch(handle: ParamHandle, testid: String, label: String, skin: Skin) -> Element {
    rsx! {
        div {
            style: "display:flex; flex-direction:column; gap:4px; align-self:center;",
            div { style: "font-size:10px; color:{skin.text}; letter-spacing:0.06em;", "{label}" }
            ParamToggle { handle, testid, skin }
        }
    }
}

/// One mic's own controls.
#[component]
fn MicSection(label: String, n: u8, kind: ParamPtr, pattern: ParamPtr, filter: ParamPtr, axis: ParamPtr, skin: Skin) -> Element {
    let ctx = use_param_context();
    rsx! {
        Section { label, skin,
            div {
                style: "display:flex; flex-direction:column; gap:10px;",
                Labeled { handle: param_handle(kind, ctx.clone()), testid: format!("type{n}"), label: "Model".to_string(), options: names(&MICS), skin, width: 140.0 }
                Labeled { handle: param_handle(pattern, ctx.clone()), testid: format!("pattern{n}"), label: "Pattern".to_string(), options: names(&PATTERNS), skin, width: 140.0 }
            }
            ParamKnob { handle: param_handle(axis, ctx.clone()), testid: format!("axis{n}") }
            ParamSelector { handle: param_handle(filter, ctx), testid: format!("filter{n}"), label: "Low Cut".to_string(), options: names(&FILTERS), skin }
        }
    }
}

/// The controls both mics share: proximity, the capsules, the output.
#[component]
fn Shared(proximity: ParamPtr, rear_trim: ParamPtr, swap: ParamPtr, source: ParamPtr, output: ParamPtr, phase: ParamPtr, skin: Skin) -> Element {
    let ctx = use_param_context();
    rsx! {
        Section { label: "Proximity".to_string(), skin,
            ParamKnob { handle: param_handle(proximity, ctx.clone()), testid: "proximity".to_string() }
        }
        Section { label: "Capsules".to_string(), skin,
            ParamKnob { handle: param_handle(rear_trim, ctx.clone()), testid: "reartrim".to_string() }
            Switch { handle: param_handle(swap, ctx.clone()), testid: "swap".to_string(), label: "Swap".to_string(), skin }
            ParamSelector { handle: param_handle(source, ctx.clone()), testid: "source".to_string(), label: "Source".to_string(), options: names(&SOURCES), skin }
        }
        Section { label: "Output".to_string(), skin,
            ParamKnob { handle: param_handle(output, ctx.clone()), testid: "output".to_string() }
            Switch { handle: param_handle(phase, ctx), testid: "phase".to_string(), label: "Phase".to_string(), skin }
        }
    }
}

/// FTS Mic's editor root.
#[component]
pub fn App() -> Element {
    rsx! {
        PluginApp { tailwind_css: include_str!("../assets/tailwind.css").to_string(), MicShell {} }
    }
}

#[component]
fn MicShell() -> Element {
    let shared = use_context::<SharedState>();
    let Some(ui) = shared.get::<MicUiState>() else { return rsx! {} };
    let ctx = use_param_context();
    let p = &ui.params;
    let skin = skin();
    let frame = use_redraw_tick();
    rsx! {
        PluginRoot {
            title: "FTS Mic".to_string(),
            subtitle: "Dual-capsule mic modeller".to_string(),
            skin,
            frame,
            ControlSurface {
                MicSection { label: "Mic 1".to_string(), n: 1, kind: p.type1.as_ptr(), pattern: p.pattern1.as_ptr(), filter: p.filter1.as_ptr(), axis: p.axis1.as_ptr(), skin }
                Section { label: "Dual".to_string(), skin,
                    Switch { handle: param_handle(p.dual.as_ptr(), ctx.clone()), testid: "dual".to_string(), label: "Dual".to_string(), skin }
                    ParamKnob { handle: param_handle(p.mix.as_ptr(), ctx.clone()), testid: "mix".to_string() }
                    ParamKnob { handle: param_handle(p.align.as_ptr(), ctx.clone()), testid: "align".to_string() }
                    ParamSelector { handle: param_handle(p.solo.as_ptr(), ctx.clone()), testid: "solo".to_string(), label: "Solo".to_string(), options: names(&SOLOS), skin }
                }
                Section { label: "180".to_string(), skin,
                    Switch { handle: param_handle(p.stereo180.as_ptr(), ctx.clone()), testid: "stereo180".to_string(), label: "180 Stereo".to_string(), skin }
                    Switch { handle: param_handle(p.link.as_ptr(), ctx.clone()), testid: "link".to_string(), label: "Link".to_string(), skin }
                    ParamKnob { handle: param_handle(p.pan.as_ptr(), ctx.clone()), testid: "pan".to_string() }
                    ParamKnob { handle: param_handle(p.width.as_ptr(), ctx), testid: "width".to_string() }
                }
                MicSection { label: "Mic 2".to_string(), n: 2, kind: p.type2.as_ptr(), pattern: p.pattern2.as_ptr(), filter: p.filter2.as_ptr(), axis: p.axis2.as_ptr(), skin }
                Shared { proximity: p.proximity.as_ptr(), rear_trim: p.rear_trim.as_ptr(), swap: p.swap.as_ptr(), source: p.source.as_ptr(), output: p.output.as_ptr(), phase: p.phase.as_ptr(), skin }
            }
        }
    }
}
