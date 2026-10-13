//! The quantize drawer — the rsx chrome over [`crate::quantize_panel`].
//!
//! State and geometry live in `quantize_panel.rs` so they are assertable
//! without a renderer; this is only the surface. Sections top to bottom
//! per r[drums.quantize.panel]: *Detect*, *Target*, *Write*, *Apply*.
//!
//! The panel is deliberately engine-blind: it holds config values and
//! emits them through `on_change` / `on_apply`, and the host runs the
//! detector and planner (`expression_editor_audio::panel_bridge`) and
//! hands the results back as `bins` and `previews`. That is what keeps
//! this crate free of the audio crate — and what makes the same panel
//! serve MIDI notes, where there is no detector at all.

use dioxus::prelude::*;
use keyboard_types::Modifiers;

use crate::quantize_panel::{
    Bin, FilterPreset, GridDivision, GridFeel, HitPreview, QuantizePanel, WriteMode,
};
use crate::theme;
use crate::widgets::Slider;

/// Mutate the panel and tell the host — every control is these two
/// steps, so a change can never render without re-detecting.
fn edit(
    mut panel: Signal<QuantizePanel>,
    on_change: EventHandler<QuantizePanel>,
    f: impl FnOnce(&mut QuantizePanel),
) {
    {
        let mut p = panel.write();
        f(&mut p);
        p.sync_config();
    }
    on_change.call(panel.read().clone());
}

/// A section heading.
fn heading(label: &str) -> Element {
    rsx! {
        div {
            style: "font-size: 9px; letter-spacing: 0.1em; text-transform: uppercase; \
                    color: {theme::TEXT_DIM}; padding: 8px 0 4px;",
            "{label}"
        }
    }
}

/// A slider that remembers its owner's preferences.
///
/// Right-click stores the current value as "my default"; Alt-click
/// resets to the stored default (or the built-in one). The same
/// affordance Perfect Timing ships, on every slider in the panel.
// r[impl drums.quantize.slider-defaults]
#[component]
fn DSlider(
    panel: Signal<QuantizePanel>,
    on_change: EventHandler<QuantizePanel>,
    /// The key the stored default is filed under.
    name: String,
    label: String,
    value: f64,
    min: f64,
    max: f64,
    /// What Alt-click falls back to when nothing was stored.
    built_in: f64,
    #[props(default)] readout: Option<String>,
    on_set: EventHandler<f64>,
) -> Element {
    let store_name = name.clone();
    let reset_name = name.clone();
    rsx! {
        div {
            "data-testid": format!("qslider-{name}"),
            oncontextmenu: move |e: MouseEvent| {
                e.prevent_default();
                edit(panel, on_change, |p| p.defaults.store(&store_name, value));
            },
            onpointerdown: move |e: PointerEvent| {
                if e.modifiers().contains(Modifiers::ALT) {
                    let v = panel.read().defaults.reset_value(&reset_name, built_in);
                    on_set.call(v);
                }
            },
            Slider {
                label,
                value,
                min,
                max,
                width: 130.0,
                readout,
                on_change: move |v: f64| on_set.call(v),
            }
        }
    }
}

/// A small pill button.
fn pill(active: bool, label: &str, testid: String, onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            style: format!("{} font-size: 10px;", theme::button_style(active)),
            "data-testid": testid,
            "data-active": "{active}",
            onclick: move |e| onclick.call(e),
            "{label}"
        }
    }
}

/// The hit histogram: where the material's hits actually sit, with the
/// sensitivity cut drawn on the same axis.
fn histogram_svg(bins: &[Bin], sensitivity: f64) -> Element {
    const W: f64 = 220.0;
    const H: f64 = 44.0;
    let peak = bins.iter().map(|b| b.count).max().unwrap_or(0).max(1) as f64;
    let n = bins.len().max(1) as f64;
    // Hits weaker than `1 - sensitivity` are excluded; the line sits at
    // that loudness on the same 0..1 axis the bins live on.
    let cut_x = (1.0 - sensitivity.clamp(0.0, 1.0)) * W;
    rsx! {
        svg {
            "data-testid": "quantize-histogram",
            view_box: "0 0 {W:.0} {H:.0}",
            style: "width: {W:.0}px; height: {H:.0}px; background: {theme::BG}; \
                    border: 1px solid {theme::PANEL_BORDER}; border-radius: 4px;",
            for (i, b) in bins.iter().enumerate() {
                rect {
                    key: "bin{i}",
                    x: "{b.from * W:.1}",
                    y: "{H - (b.count as f64 / peak) * (H - 4.0):.1}",
                    width: "{(W / n - 1.0).max(1.0):.1}",
                    height: "{(b.count as f64 / peak) * (H - 4.0):.1}",
                    fill: if b.from >= 1.0 - sensitivity { theme::ACCENT } else { theme::CONTROL_GROOVE },
                }
            }
            line {
                x1: "{cut_x:.1}", y1: "0", x2: "{cut_x:.1}", y2: "{H:.0}",
                stroke: theme::GOLD, stroke_width: "1",
            }
        }
    }
}

/// The compact preview strip: each hit, its planned position, and the
/// arrow between them, coloured by how far it moves. Excluded hits are
/// dimmed, never hidden — "why did that hit not move" must stay
/// answerable.
// r[impl drums.quantize.preview]
fn preview_strip(previews: &[HitPreview], grid: f64) -> Element {
    const W: f64 = 220.0;
    const H: f64 = 26.0;
    let (from, to) = previews.iter().fold((f64::MAX, f64::MIN), |(a, b), p| {
        (a.min(p.at.min(p.to)), b.max(p.at.max(p.to)))
    });
    let span = (to - from).max(1e-9);
    let x_of = move |t: f64| ((t - from) / span) * (W - 8.0) + 4.0;
    let moved = previews.iter().filter(|p| p.moved).count();
    let excluded = previews.iter().filter(|p| p.excluded).count();
    rsx! {
        div {
            "data-testid": "quantize-preview",
            style: "display: flex; flex-direction: column; gap: 3px;",
            svg {
                view_box: "0 0 {W:.0} {H:.0}",
                style: "width: {W:.0}px; height: {H:.0}px; background: {theme::BG}; \
                        border: 1px solid {theme::PANEL_BORDER}; border-radius: 4px;",
                for (i, p) in previews.iter().enumerate() {
                    line {
                        key: "hit{i}",
                        x1: "{x_of(p.at):.1}", y1: "4",
                        x2: "{x_of(p.at):.1}", y2: "{H - 4.0:.1}",
                        stroke: if p.excluded { theme::TEXT_FAINT } else { theme::TEXT },
                        stroke_width: if p.excluded { "1" } else { "1.5" },
                    }
                    if p.moved {
                        line {
                            key: "arrow{i}",
                            x1: "{x_of(p.at):.1}", y1: "{H * 0.5:.1}",
                            x2: "{x_of(p.to):.1}", y2: "{H * 0.5:.1}",
                            stroke: shift_color((p.to - p.at).abs(), grid),
                            stroke_width: "2",
                        }
                    }
                }
            }
            span {
                style: "font-size: 9px; color: {theme::TEXT_DIM};",
                "{previews.len()} hits · {moved} move · {excluded} left alone"
            }
        }
    }
}

/// Colour by how far a hit moves, as a fraction of the grid.
fn shift_color(shift: f64, grid: f64) -> &'static str {
    let frac = if grid > 0.0 { shift / grid } else { 0.0 };
    let i = ((frac * 8.0) as usize).min(theme::TUNE_RAMP.len() - 1);
    theme::TUNE_RAMP[i]
}

/// The quantize drawer.
// r[impl drums.quantize.panel]
#[component]
pub fn QuantizePanelView(
    panel: Signal<QuantizePanel>,
    /// Histogram bins from the host's current detection.
    #[props(default)]
    bins: Vec<Bin>,
    /// Per-hit from→to moves from the host's current plan.
    #[props(default)]
    previews: Vec<HitPreview>,
    /// Every control change lands here so the host re-detects and
    /// re-plans immediately. Nothing is written until Apply.
    on_change: EventHandler<QuantizePanel>,
    /// Apply — host-provided. The standalone/REAPER hosts write the
    /// plan through the group rule in one undo step
    /// (r[drums.quantize.apply], r[drums.quantize.undo]); the panel
    /// never writes.
    on_apply: EventHandler<QuantizePanel>,
) -> Element {
    let p = panel.read().clone();
    let ms = |s: f64| format!("{:.0} ms", s * 1000.0);

    rsx! {
        div {
            "data-testid": "quantize-panel",
            style: "position: absolute; top: 0; right: 0; bottom: 0; width: 268px; \
                    z-index: 20; display: flex; flex-direction: column; \
                    box-sizing: border-box; padding: 8px 10px; gap: 2px; \
                    background: {theme::PANEL}; border-left: 1px solid {theme::PANEL_BORDER}; \
                    color: {theme::TEXT}; font-family: system-ui, sans-serif; \
                    font-size: 11px; overflow-y: auto;",

            div {
                style: "display: flex; align-items: center; justify-content: space-between;",
                span {
                    style: "font-size: 10px; letter-spacing: 0.08em; \
                            text-transform: uppercase; color: {theme::TEXT_DIM};",
                    "Quantize"
                }
            }

            // ── Detect ───────────────────────────────────────────────
            {heading("Detect")}
            {histogram_svg(&bins, p.detect.sensitivity)}
            DSlider {
                panel, on_change,
                name: "threshold".to_string(),
                label: "Thresh".to_string(),
                value: p.detect.threshold_db,
                min: -80.0, max: -20.0,
                built_in: -60.0,
                readout: format!("{:.0} dB", p.detect.threshold_db),
                on_set: move |v: f64| edit(panel, on_change, |p| p.detect.threshold_db = v),
            }
            DSlider {
                panel, on_change,
                name: "sensitivity".to_string(),
                label: "Sens".to_string(),
                value: p.detect.sensitivity,
                min: 0.0, max: 1.0,
                built_in: 0.5,
                readout: format!("{:.0} %", p.detect.sensitivity * 100.0),
                on_set: move |v: f64| edit(panel, on_change, |p| p.detect.sensitivity = v),
            }

            // The filter preset combo: one pick instead of four sliders.
            // r[impl drums.quantize.filter-presets]
            div {
                style: "display: flex; align-items: center; gap: 4px; padding: 2px 0;",
                span { style: "min-width: 40px; font-size: 10px; color: {theme::TEXT_DIM};", "Filter" }
                button {
                    style: format!("{} min-width: 72px; font-size: 10px;", theme::button_style(false)),
                    "data-testid": "quantize-preset",
                    title: "Detection filter preset — click to cycle",
                    onclick: move |_| edit(panel, on_change, |p| {
                        let next = (p.preset + 1) % p.presets.len().max(1);
                        p.apply_preset(next);
                    }),
                    "{p.presets.get(p.preset).map(|f| f.name.as_str()).unwrap_or(\"—\")}"
                }
                button {
                    style: format!("{} font-size: 10px;", theme::button_style(false)),
                    "data-testid": "quantize-preset-save",
                    title: "Save the current filters as a preset",
                    onclick: move |_| edit(panel, on_change, |p| {
                        let n = p.presets.len() + 1 - FilterPreset::builtins().len();
                        p.save_preset(format!("User {n}"));
                    }),
                    "+"
                }
            }

            // Advanced: the controls that are set once per kit.
            {pill(p.advanced, if p.advanced { "▾ advanced" } else { "▸ advanced" },
                  "quantize-advanced".to_string(),
                  EventHandler::new(move |_| edit(panel, on_change, |p| p.advanced = !p.advanced)))}
            if p.advanced {
                DSlider {
                    panel, on_change,
                    name: "crest".to_string(),
                    label: "Crest".to_string(),
                    value: p.detect.crest_db,
                    min: 0.0, max: 24.0,
                    built_in: 3.0,
                    readout: format!("{:.1} dB", p.detect.crest_db),
                    on_set: move |v: f64| edit(panel, on_change, |p| p.detect.crest_db = v),
                }
                DSlider {
                    panel, on_change,
                    name: "low-cut".to_string(),
                    label: "Low cut".to_string(),
                    value: p.detect.high_pass_hz.unwrap_or(0.0),
                    min: 0.0, max: 500.0,
                    built_in: 0.0,
                    readout: p.detect.high_pass_hz.map(|hz| format!("{hz:.0} Hz")).unwrap_or_else(|| "off".to_string()),
                    on_set: move |v: f64| edit(panel, on_change, |p| {
                        p.detect.high_pass_hz = (v >= 1.0).then_some(v);
                    }),
                }
                DSlider {
                    panel, on_change,
                    name: "high-cut".to_string(),
                    label: "High cut".to_string(),
                    value: p.detect.low_pass_hz.unwrap_or(20_000.0),
                    min: 100.0, max: 20_000.0,
                    built_in: 20_000.0,
                    readout: p.detect.low_pass_hz.map(|hz| format!("{hz:.0} Hz")).unwrap_or_else(|| "off".to_string()),
                    on_set: move |v: f64| edit(panel, on_change, |p| {
                        p.detect.low_pass_hz = (v <= 19_999.0).then_some(v);
                    }),
                }
                DSlider {
                    panel, on_change,
                    name: "retrigger".to_string(),
                    label: "Retrig".to_string(),
                    value: p.detect.retrigger_secs,
                    min: 0.005, max: 0.200,
                    built_in: 0.050,
                    readout: ms(p.detect.retrigger_secs),
                    on_set: move |v: f64| edit(panel, on_change, |p| p.detect.retrigger_secs = v),
                }
                DSlider {
                    panel, on_change,
                    name: "offset".to_string(),
                    label: "Offset".to_string(),
                    value: p.detect.time_offset_secs,
                    min: -0.030, max: 0.030,
                    built_in: 0.0,
                    readout: format!("{:+.1} ms", p.detect.time_offset_secs * 1000.0),
                    on_set: move |v: f64| edit(panel, on_change, |p| p.detect.time_offset_secs = v),
                }
            }

            // ── Target ───────────────────────────────────────────────
            {heading("Target")}
            // r[impl drums.quantize.grid-options]
            div {
                style: "display: flex; flex-wrap: wrap; gap: 3px;",
                for d in GridDivision::ALL {
                    {pill(p.division == d, d.label(),
                          format!("quantize-div-{}", d.label().replace('/', "-")),
                          EventHandler::new(move |_| edit(panel, on_change, |p| p.division = d)))}
                }
            }
            div {
                style: "display: flex; gap: 3px; padding: 2px 0;",
                for f in GridFeel::ALL {
                    {pill(p.feel == f, f.label(),
                          format!("quantize-feel-{}", f.label()),
                          EventHandler::new(move |_| edit(panel, on_change, |p| p.feel = f)))}
                }
            }
            DSlider {
                panel, on_change,
                name: "swing".to_string(),
                label: "Swing".to_string(),
                value: p.swing,
                min: 0.0, max: 1.0,
                built_in: 0.0,
                readout: format!("{:.0} %", p.swing * 100.0),
                on_set: move |v: f64| edit(panel, on_change, |p| p.swing = v),
            }
            div {
                style: "display: flex; align-items: center; gap: 4px;",
                {pill(p.grid_scan, "grid scan", "quantize-grid-scan".to_string(),
                      EventHandler::new(move |_| edit(panel, on_change, |p| p.grid_scan = !p.grid_scan)))}
                span {
                    style: "font-size: 9px; color: {theme::TEXT_DIM};",
                    if p.grid_scan { "loudest hit per division" } else { "every hit to its nearest" }
                }
            }
            if p.grid_scan {
                DSlider {
                    panel, on_change,
                    name: "tolerance".to_string(),
                    label: "Toler".to_string(),
                    value: p.tolerance,
                    min: 0.001, max: 0.200,
                    built_in: 0.05,
                    readout: ms(p.tolerance),
                    on_set: move |v: f64| edit(panel, on_change, |p| p.tolerance = v),
                }
            }
            DSlider {
                panel, on_change,
                name: "strength".to_string(),
                label: "Strength".to_string(),
                value: p.config.strength,
                min: 0.0, max: 1.0,
                built_in: 1.0,
                readout: format!("{:.0} %", p.config.strength * 100.0),
                on_set: move |v: f64| edit(panel, on_change, |p| p.config.strength = v),
            }

            // ── Write ────────────────────────────────────────────────
            {heading("Write")}
            div {
                style: "display: flex; gap: 3px;",
                {pill(p.mode == WriteMode::Split, "SPLIT", "quantize-mode-split".to_string(),
                      EventHandler::new(move |_| edit(panel, on_change, |p| p.mode = WriteMode::Split)))}
                {pill(p.mode == WriteMode::Warp, "WARP", "quantize-mode-warp".to_string(),
                      EventHandler::new(move |_| edit(panel, on_change, |p| p.mode = WriteMode::Warp)))}
            }
            if p.mode == WriteMode::Split {
                DSlider {
                    panel, on_change,
                    name: "pad".to_string(),
                    label: "Pad".to_string(),
                    value: p.pad,
                    min: 0.0, max: 0.030,
                    built_in: 0.007,
                    readout: ms(p.pad),
                    on_set: move |v: f64| edit(panel, on_change, |p| p.pad = v),
                }
                DSlider {
                    panel, on_change,
                    name: "crossfade".to_string(),
                    label: "Xfade".to_string(),
                    value: p.crossfade,
                    min: 0.0, max: 0.030,
                    built_in: 0.007,
                    readout: ms(p.crossfade),
                    on_set: move |v: f64| edit(panel, on_change, |p| p.crossfade = v),
                }
            }

            // ── The plan, and Apply ──────────────────────────────────
            if !previews.is_empty() {
                {heading("Plan")}
                {preview_strip(&previews, p.config.grid)}
            }
            div {
                style: "margin-top: auto; padding-top: 10px;",
                button {
                    style: format!("{} width: 100%;", theme::button_style(true)),
                    "data-testid": "quantize-apply",
                    // r[impl drums.quantize.apply]
                    onclick: move |_| on_apply.call(panel.read().clone()),
                    "Apply"
                }
            }
        }
    }
}
