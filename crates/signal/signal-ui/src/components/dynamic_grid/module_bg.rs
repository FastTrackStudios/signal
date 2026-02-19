//! Container background components — visual-only backgrounds with title bars.
//!
//! Supports three nesting levels: Engine (outermost), Layer (middle), Module (innermost).

use audio_controls::widgets::VSlider;
use dioxus::prelude::*;

use super::layout::{ContainerLevel, ENGINE_TITLE_H, GROUP_TITLE_H, LAYER_LEFT_PAD};
use super::types::ModuleVisualState;

// ─────────────────────────────────────────────────────────────────────────────
// Module-level background (innermost, existing)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
pub(super) struct ModuleBackgroundProps {
    pub name: String,
    pub bg_color: String,
    pub fg_color: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub visual_state: ModuleVisualState,
}

#[component]
pub(super) fn ModuleBackground(props: ModuleBackgroundProps) -> Element {
    let bg = format!(
        "left: {}px; top: {}px; width: {}px; height: {}px; \
         background-color: {}12; border: 1px solid {}30; border-radius: 10px;",
        props.x, props.y, props.w, props.h, props.bg_color, props.bg_color,
    );
    let title_style = format!(
        "background-color: {}20; border-bottom: 1px solid {}25; \
         border-radius: 10px 10px 0 0; height: {}px;",
        props.bg_color, props.bg_color, GROUP_TITLE_H,
    );
    let opacity = props.visual_state.opacity();
    let extra_style = props.visual_state.extra_style();
    let transition = props.visual_state.transition();
    let selection_glow = props.visual_state.selection_glow(&props.bg_color);

    rsx! {
        div {
            key: "grp-{props.name}",
            class: "absolute overflow-hidden",
            style: "position: absolute; {bg} z-index: 3; pointer-events: none; opacity: {opacity}; transition: {transition}; {extra_style} {selection_glow}",
            div {
                class: "flex items-center gap-1.5 px-2",
                style: "{title_style} pointer-events: none;",
                div {
                    class: "w-2 h-2 rounded-full flex-shrink-0",
                    style: "background-color: {props.bg_color};",
                }
                span {
                    class: "text-[8px] font-semibold tracking-wide whitespace-nowrap opacity-80",
                    style: "color: {props.fg_color};",
                    "{props.name}"
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Container background (generic for Engine/Layer levels)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
pub(super) struct ContainerBackgroundProps {
    pub name: String,
    pub bg_color: String,
    pub fg_color: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub level: ContainerLevel,
}

#[component]
pub(super) fn ContainerBackground(props: ContainerBackgroundProps) -> Element {
    // Each level has its own structural space allocated in the bounding box,
    // so labels at different nesting depths can never collide.
    // Engine: top title strip. Layer: left side label (rotated).
    let (bg_alpha, border_alpha, border_style, radius, z_index) = match props.level {
        ContainerLevel::Engine => ("0a", "18", "solid", "10px", 1),
        ContainerLevel::Layer => ("08", "14", "dashed", "8px", 2),
        ContainerLevel::Module => ("12", "30", "solid", "10px", 3),
    };

    let bg = format!(
        "left: {}px; top: {}px; width: {}px; height: {}px; \
         background-color: {}{bg_alpha}; border: 1px {border_style} {}{border_alpha}; border-radius: {radius};",
        props.x, props.y, props.w, props.h, props.bg_color, props.bg_color,
    );

    let is_layer = props.level == ContainerLevel::Layer;

    // Layer side panel needs a local volume signal (visual prototype, not yet
    // wired to domain model).
    let volume = use_signal(|| 1.0f32);

    // Fader height: full container minus top/bottom padding.
    let fader_h = (props.h - 8.0).max(20.0) as u32;

    rsx! {
        div {
            key: "container-{props.name}",
            class: "absolute",
            style: "position: absolute; {bg} z-index: {z_index}; pointer-events: none;",
            if is_layer {
                // Left side panel: rotated name + volume fader, side by side
                div {
                    style: "position: absolute; left: 0; top: 0; width: {LAYER_LEFT_PAD}px; height: {props.h}px; \
                            display: flex; flex-direction: row; align-items: stretch; \
                            padding: 4px 2px; gap: 0px; pointer-events: auto;",
                    // Rotated layer name on the far left
                    div {
                        style: "display: flex; align-items: center; justify-content: center; width: 14px; flex-shrink: 0;",
                        span {
                            class: "text-[7px] font-medium tracking-wide whitespace-nowrap",
                            style: "color: {props.fg_color}; opacity: 0.60; \
                                    writing-mode: vertical-lr; transform: rotate(180deg);",
                            "{props.name}"
                        }
                    }
                    // Volume fader to the right of the name
                    div {
                        style: "display: flex; align-items: center; justify-content: center; flex: 1;",
                        VSlider {
                            value: volume,
                            height: fader_h,
                            width: 16,
                            min: 0.0,
                            max: 1.0,
                        }
                    }
                }
            } else {
                // Top title strip for Engine (and Module fallback)
                {
                    let title_h = match props.level {
                        ContainerLevel::Engine => ENGINE_TITLE_H,
                        ContainerLevel::Module => GROUP_TITLE_H,
                        _ => 0.0,
                    };
                    let font_class = match props.level {
                        ContainerLevel::Engine => "text-[7px] font-semibold uppercase tracking-wider",
                        _ => "text-[8px] font-semibold",
                    };
                    let label_opacity = match props.level {
                        ContainerLevel::Engine => "0.50",
                        _ => "0.80",
                    };
                    rsx! {
                        div {
                            class: "flex items-center px-1.5",
                            style: "height: {title_h}px; pointer-events: none;",
                            span {
                                class: "{font_class} whitespace-nowrap",
                                style: "color: {props.fg_color}; opacity: {label_opacity};",
                                "{props.name}"
                            }
                        }
                    }
                }
            }
        }
    }
}
