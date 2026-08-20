//! The stack strip — one chip per stage, lanes separated (`fx.stack.strip`).
//!
//! Shown whenever the stack has more than one stage. Chips are in topology
//! order: lanes left to right, each lane's serial stages left to right,
//! separated by a lane divider carrying the lane's mute/solo. Gestures:
//!
//! - click a chip: focus that stage (`fx.stack.focus`)
//! - Alt-click a chip: bypass / enable the stage (crossfaded)
//! - click a chip's ×: remove the stage from the stack
//! - click a lane's M/S dots: mute / solo the lane
//!
//! Everything a chip writes is a host parameter (`fx.stack.params`), through
//! the same gesture path as any knob.

use audiocore_core::prelude::*;

use crate::focus::FocusedStage;
use crate::params::{CompParams, CompUiState, MAX_STAGES};

/// Write a bool param as one gesture.
fn set_bool(ctx: &ParamContext, param: &BoolParam, v: bool) {
    let ptr = param.as_ptr();
    ctx.begin_set_raw(ptr);
    ctx.set_normalized_raw(ptr, if v { 1.0 } else { 0.0 });
    ctx.end_set_raw(ptr);
}

/// Write an int param (plain value) as one gesture.
fn set_int(ctx: &ParamContext, param: &IntParam, v: i32) {
    let ptr = param.as_ptr();
    ctx.begin_set_raw(ptr);
    ctx.set_normalized_raw(ptr, param.preview_normalized(v));
    ctx.end_set_raw(ptr);
}

/// Add a stage of `profile_index` to the stack: the first free pool slot, on
/// `lane`, focused (`fx.stack.add`). No-op when the pool is full.
pub fn add_stage(
    params: &std::sync::Arc<CompParams>,
    ctx: &ParamContext,
    mut focus: Signal<FocusedStage>,
    profile_index: usize,
    lane: usize,
) {
    let Some(slot) = params.first_free_stage() else {
        return;
    };
    let stage = params.stage(slot);
    set_int(ctx, &stage.profile, profile_index as i32);
    stage.store_profile_id(profile_index);
    set_int(ctx, &stage.lane, lane as i32);
    set_bool(ctx, &stage.stage_on, true);
    set_bool(ctx, &stage.in_use, true);
    params.store_focused_stage(slot);
    focus.set(FocusedStage(slot));
}

/// Remove stage `idx` from the stack; focus moves to the first remaining
/// stage. The last stage cannot be removed (`fx.stack.strip`).
pub fn remove_stage(
    params: &std::sync::Arc<CompParams>,
    ctx: &ParamContext,
    mut focus: Signal<FocusedStage>,
    idx: usize,
) {
    if params.stages_in_use().len() <= 1 {
        return;
    }
    set_bool(ctx, &params.stage(idx).in_use, false);
    if focus.peek().0 == idx {
        let next = (0..MAX_STAGES)
            .find(|&i| i != idx && params.stage(i).in_use.value())
            .unwrap_or(0);
        params.store_focused_stage(next);
        focus.set(FocusedStage(next));
    }
}

/// The strip itself. Renders nothing while the stack is a single stage.
#[component]
pub fn StackStrip(frame: u64) -> Element {
    let _ = frame;
    let shared = use_context::<SharedState>();
    let ui = shared.get::<CompUiState>().expect("CompUiState missing");
    let ctx = use_param_context();
    let params = ui.params.clone();
    let focus = crate::focus::use_focus_signal();
    let Some(focus) = focus else { return rsx! {} };

    let in_use = params.stages_in_use();
    if in_use.len() <= 1 {
        return rsx! {};
    }
    let focused = focus.read().0;

    // Lanes in index order, each with its stages in pool order.
    let mut lanes: Vec<(usize, Vec<usize>)> = Vec::new();
    for &si in &in_use {
        let lane = params.stage(si).lane.value().max(0) as usize;
        match lanes.iter_mut().find(|(l, _)| *l == lane) {
            Some((_, v)) => v.push(si),
            None => lanes.push((lane, vec![si])),
        }
    }
    lanes.sort_by_key(|(l, _)| *l);
    let parallel = lanes.len() > 1;

    rsx! {
        div {
            "data-testid": "stack-strip",
            // Centred without a transform, stacked without z-index — both
            // break blitz hit-testing: transforms are not applied to hit
            // coordinates, and a z-indexed child is hoisted into a stacking
            // context whose hit area only exists after a real paint (never in
            // the headless tests). Document order does the stacking: the
            // strip is the LAST sibling of the face, so it paints and hits on
            // top.
            style: "position:absolute; top:4px; right:12px; \
                    width:max-content; \
                    display:flex; align-items:center; gap:6px; \
                    padding:4px 8px; border-radius:8px; \
                    background:color-mix(in oklab, var(--card, #101216) 88%, transparent); \
                    border:1px solid var(--border, rgba(148,163,184,0.3));",

            for (lane_pos, (lane, stages)) in lanes.iter().enumerate() {
                // Lane divider + mute/solo, before every lane but the first.
                if lane_pos > 0 {
                    div {
                        style: "width:1px; height:22px; \
                                background:var(--border, rgba(148,163,184,0.4));",
                    }
                }
                if parallel {
                    {
                        let lp = &params.lanes[*lane];
                        let muted = lp.mute.value();
                        let soloed = lp.solo.value();
                        let ctx_m = ctx.clone();
                        let ctx_s = ctx.clone();
                        let params_m = params.clone();
                        let params_s = params.clone();
                        let lane_m = *lane;
                        let lane_s = *lane;
                        rsx! {
                            div {
                                style: "display:flex; flex-direction:column; gap:2px;",
                                span {
                                    "data-testid": "lane-mute-{lane}",
                                    style: format!(
                                        "font-size:8px; font-weight:800; cursor:pointer; \
                                         padding:0 3px; border-radius:3px; color:{}; background:{};",
                                        if muted { "#0b0b0d" } else { "var(--muted-foreground)" },
                                        if muted { "#e5484d" } else { "transparent" },
                                    ),
                                    title: "Mute lane",
                                    onmousedown: move |_| {
                                        let lp = &params_m.lanes[lane_m];
                                        set_bool(&ctx_m, &lp.mute, !lp.mute.value());
                                    },
                                    "M"
                                }
                                span {
                                    "data-testid": "lane-solo-{lane}",
                                    style: format!(
                                        "font-size:8px; font-weight:800; cursor:pointer; \
                                         padding:0 3px; border-radius:3px; color:{}; background:{};",
                                        if soloed { "#0b0b0d" } else { "var(--muted-foreground)" },
                                        if soloed { "#f5d90a" } else { "transparent" },
                                    ),
                                    title: "Solo lane",
                                    onmousedown: move |_| {
                                        let lp = &params_s.lanes[lane_s];
                                        set_bool(&ctx_s, &lp.solo, !lp.solo.value());
                                    },
                                    "S"
                                }
                            }
                        }
                    }
                }

                for &si in stages.iter() {
                    {
                        let stage = params.stage(si);
                        let profile_idx = stage.resolved_profile_index();
                        let badge = crate::faces::profile_badge(
                            crate::faces::profile_id_for_index(profile_idx),
                        );
                        let enabled = stage.stage_on.value();
                        let is_focused = si == focused;
                        let ctx_chip = ctx.clone();
                        let params_chip = params.clone();
                        let ctx_x = ctx.clone();
                        let params_x = params.clone();
                        let mut focus_chip = focus;
                        let focus_x = focus;
                        rsx! {
                            div {
                                "data-testid": "stage-chip-{si + 1}",
                                "data-focused": "{is_focused}",
                                "data-enabled": "{enabled}",
                                title: format!(
                                    "Stage {} — {} · click: edit · Alt-click: bypass",
                                    si + 1,
                                    crate::params::PROFILE_LABELS
                                        .get(profile_idx)
                                        .copied()
                                        .unwrap_or("?"),
                                ),
                                style: format!(
                                    "display:flex; align-items:center; gap:4px; cursor:pointer; \
                                     padding:3px 6px; border-radius:6px; font-size:10px; \
                                     font-weight:{}; letter-spacing:0.02em; \
                                     color:{}; background:{}; border:1px solid {}; opacity:{};",
                                    if is_focused { 800 } else { 600 },
                                    if is_focused { "#0b0b0d" } else { "var(--foreground)" },
                                    if is_focused {
                                        "var(--primary, #c8a86e)"
                                    } else {
                                        "color-mix(in oklab, var(--primary, #c8a86e) 12%, transparent)"
                                    },
                                    if is_focused {
                                        "var(--primary, #c8a86e)"
                                    } else {
                                        "var(--border, rgba(148,163,184,0.3))"
                                    },
                                    if enabled { "1.0" } else { "0.5" },
                                ),
                                // mousedown, not click: the chip floats over
                                // the face, and blitz's click synthesis is
                                // unreliable for overlapped floats freshly
                                // hovered — mousedown dispatches straight to
                                // the hit target. The remove × below stops
                                // propagation on ITS mousedown so a remove
                                // does not also refocus.
                                onmousedown: move |evt: MouseEvent| {
                                    if evt.modifiers().alt() {
                                        // Bypass toggle, crossfaded downstream.
                                        let s = params_chip.stage(si);
                                        set_bool(&ctx_chip, &s.stage_on, !s.stage_on.value());
                                        return;
                                    }
                                    params_chip.store_focused_stage(si);
                                    focus_chip.set(FocusedStage(si));
                                },
                                "{badge}"
                                span {
                                    "data-testid": "stage-remove-{si + 1}",
                                    style: "font-size:9px; opacity:0.7; cursor:pointer; padding:0 1px;",
                                    title: "Remove stage",
                                    onmousedown: move |evt: MouseEvent| {
                                        evt.stop_propagation();
                                        remove_stage(&params_x, &ctx_x, focus_x, si);
                                    },
                                    "×"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
