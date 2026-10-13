//! Rotary knob — a painted dial with the shared gesture layer.
//!
//! Provider-agnostic port of `audio-gui::controls::knob::Knob`. Binds to a
//! [`ParamHandle`] instead of a `nih_plug::ParamPtr`. Requires a
//! [`crate::drag::DragProvider`] ancestor for drag capture.
//!
//! The dial is painted by [`crate::paint::knob`] into an anyrender scene and
//! shown through a Blitz custom widget (`fx.control.painted`) — not an inline
//! `<svg>`, which Blitz rescales as a replaced element and re-parses on every
//! value change. The wasm build (no custom widgets) keeps a minimal svg
//! fallback drawn from the same geometry.

use crate::color::css_color_to_hex;
use crate::controls::readout::ValueReadout;
use crate::drag::DragState;
use crate::gesture::{self, Press, KNOB_SENSITIVITY};
use crate::paint::knob::KnobLook;
use crate::param::ParamHandle;
use crate::theme::*;
use dioxus::prelude::*;

/// Display size.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum KnobSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl KnobSize {
    pub fn diameter(self) -> u32 {
        match self {
            Self::Small => 44,
            Self::Medium => 56,
            Self::Large => 72,
        }
    }
}

/// Resolve the architect-ui theme tokens the dial is drawn with into concrete
/// colours. Falls back to the dark palette when no `ThemeProvider` is in
/// scope (unit tests).
fn resolve_theme(key: &str, fallback: &str) -> String {
    let theme = try_consume_context::<architect_ui::prelude::ThemeContext>();
    theme
        .as_ref()
        .and_then(|ctx| {
            let state = ctx.state.read();
            let style = state.styles.active(state.mode);
            style.get(key).map(str::to_string)
        })
        .and_then(|raw| css_color_to_hex(&raw))
        .unwrap_or_else(|| fallback.to_string())
}

/// The dial alone: a `diameter × diameter` box painted from `look`.
///
/// Native: an `<object>` carrying a [`crate::widget::SceneWidget`] that
/// replays the scene this component builds on every render. The element is
/// `display:block` with an explicit box — a widget reports no intrinsic size
/// and blitz-paint skips a zero-box widget silently.
#[component]
pub fn KnobDial(look: KnobLook) -> Element {
    let d = look.diameter;
    #[cfg(not(target_arch = "wasm32"))]
    {
        let painted = crate::widget::use_painted();
        painted.slot.put(crate::paint::knob::scene(&look));
        rsx! {
            object {
                "data": painted.widget.clone(),
                style: "display:block; width:{d}px; height:{d}px; pointer-events:none;",
            }
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        // No custom widgets in the browser renderer: the same geometry as
        // svg, value arc + pointer only. Kept deliberately minimal so it is
        // the painter that gets the design work.
        use crate::paint::knob::{angle_for_value, arc_point, START_ANGLE, SWEEP};
        let cx = d / 2.0;
        let r = d / 2.0 - 4.0;
        let svg_arc = |r: f64, a: f64, b: f64| {
            let (x1, y1) = arc_point(cx, cx, r, a);
            let (x2, y2) = arc_point(cx, cx, r, b);
            let large = if (b - a).abs() > 180.0 { 1 } else { 0 };
            format!("M {x1:.1} {y1:.1} A {r:.1} {r:.1} 0 {large} 1 {x2:.1} {y2:.1}")
        };
        let track = svg_arc(r, START_ANGLE, START_ANGLE + SWEEP);
        let end = angle_for_value(look.value);
        let value = if look.bipolar {
            let c = START_ANGLE + SWEEP / 2.0;
            if look.value > 0.5 {
                svg_arc(r, c, end)
            } else {
                svg_arc(r, end, c)
            }
        } else {
            svg_arc(r, START_ANGLE, end)
        };
        let (tx, ty) = arc_point(cx, cx, r - 6.0, end);
        let (tx2, ty2) = arc_point(cx, cx, r + 1.0, end);
        let hex = |c: peniko::Color| {
            let [r, g, b, _] = c.to_rgba8().to_u8_array();
            format!("#{r:02x}{g:02x}{b:02x}")
        };
        let accent = hex(look.accent);
        let track_c = hex(look.track);
        let pointer = hex(look.pointer);
        rsx! {
            svg {
                width: "{d}", height: "{d}", view_box: "0 0 {d} {d}",
                path { d: "{track}", fill: "none", stroke: "{track_c}", stroke_width: "4", stroke_linecap: "round" }
                path { d: "{value}", fill: "none", stroke: "{accent}", stroke_width: "4.5", stroke_linecap: "round" }
                line { x1: "{tx:.1}", y1: "{ty:.1}", x2: "{tx2:.1}", y2: "{ty2:.1}", stroke: "{pointer}", stroke_width: "2.25", stroke_linecap: "round" }
            }
        }
    }
}

/// A rotary knob bound to a [`ParamHandle`].
///
/// ## Interaction
///
/// Every gesture goes through [`crate::gesture`] (spec `fx.control.*`):
///
/// - **Vertical drag**: relative adjustment, 150 px per full sweep.
/// - **Ctrl/Cmd + drag** or **Shift + drag**: fine (×8); both: ultra-fine (×32).
/// - **Mouse wheel**: stepped adjustment; same modifiers for finer steps.
/// - **Double-click** / **Alt + click**: reset to the default value.
/// - **Click the readout**: type a value (`1k`, `A4`, `2x`, `50%`…).
/// - **Right-click**: open text entry.
/// - Focused: arrows / PgUp / PgDn / Home / End / Backspace / Enter.
///
/// ## Visualization
///
/// - Default: value arc sweeps from the start of the track toward the cursor.
/// - Bipolar (mark via [`ParamHandle::with_bipolar`]): value arc sweeps from
///   the centre detent (12 o'clock) outward in either direction. Better for
///   gain / pan / cents-style params.
#[component]
pub fn Knob(
    /// The parameter this knob drives.
    handle: ParamHandle,
    #[props(default)] size: KnobSize,
    /// Override the parameter's name when rendering the label.
    #[props(default)]
    label: Option<String>,
    /// Accent color override (e.g. `"#F97316"`).
    #[props(default)]
    color: Option<String>,
    /// Modulation range minimum (0.0–1.0). Drawn as an overlay arc.
    #[props(default)]
    mod_min: Option<f64>,
    #[props(default)] mod_max: Option<f64>,
    #[props(default)] disabled: bool,
) -> Element {
    let mut drag: Signal<DragState> = use_context();
    let mut editing = use_signal(|| false);
    let mut hovered = use_signal(|| false);

    // Re-render while a drag is active so the value display tracks the cursor.
    let _ = drag.read().move_count;

    let param_name = label.unwrap_or_else(|| handle.name());
    let is_hovered = *hovered.read();
    let d = size.diameter();

    let accent = color
        .as_deref()
        .map(str::to_string)
        .unwrap_or_else(|| resolve_theme("primary", "#c8a86e"));
    let look = KnobLook {
        diameter: d as f64,
        value: handle.normalized() as f64,
        bipolar: handle.is_bipolar(),
        mod_range: match (mod_min, mod_max) {
            (Some(lo), Some(hi)) => Some((lo, hi)),
            _ => None,
        },
        active: !disabled && (is_hovered || drag.read().active),
        accent: crate::paint::color(&accent),
        track: crate::paint::color(&resolve_theme("border", "#2a2a30")),
        pointer: crate::paint::color(&resolve_theme("foreground", "#d4d4d8")),
        detent: crate::paint::color(&resolve_theme("muted-foreground", "#737380")),
        mod_color: crate::paint::color(&resolve_theme("accent", "#8b5cf6")),
        cap_fill: crate::paint::color(&resolve_theme("card", "#0c0c0f")),
        cap_stroke: crate::paint::color(&resolve_theme("border", "#2a2a30")),
    };

    let opacity = if disabled { "0.5" } else { "1.0" };
    let cursor = if disabled { "not-allowed" } else { "pointer" };
    // Hover/drag visual: a gentle scale (the glow is painted).
    let scale = if look.active { "1.04" } else { "1.0" };

    rsx! {
        div {
            style: format!(
                "display:inline-flex; flex-direction:column; align-items:center; gap:6px; \
                 opacity:{opacity}; cursor:{cursor}; position:relative; \
                 transform:scale({scale}); transition:transform 90ms ease-out; \
                 padding:2px 0; \
                 color:var(--foreground);"
            ),
            title: format!("{param_name} — {}\nDrag · Ctrl/Shift=fine · Wheel · Dbl-click=reset · Click value=type", handle.display_value()),
            onmouseenter: move |_| { hovered.set(true); },
            onmouseleave: move |_| { hovered.set(false); },

            // The dial and its gesture overlay share one box, so the overlay
            // covers the dial and nothing else — the readout below must stay
            // clickable for text entry (`fx.control.text-entry`).
            div {
                style: "position:relative; width:{d}px; height:{d}px;",

                KnobDial { look }

                if !disabled {
                    div {
                        style: "position:absolute; inset:0; cursor:ns-resize; user-select:none;",
                        tabindex: "0",
                        onmousedown: {
                            let handle = handle.clone();
                            move |evt: MouseEvent| {
                                if gesture::press_vertical(&evt, &mut drag, &handle, KNOB_SENSITIVITY)
                                    == Press::Menu
                                {
                                    editing.set(true);
                                }
                            }
                        },
                        ondoubleclick: {
                            let handle = handle.clone();
                            move |_| gesture::double_click(&mut drag, &handle)
                        },
                        onwheel: {
                            let handle = handle.clone();
                            move |evt: WheelEvent| gesture::wheel(&evt, &handle)
                        },
                        onkeydown: {
                            let handle = handle.clone();
                            move |evt: KeyboardEvent| {
                                if gesture::key(&evt, &handle) == gesture::KeyOutcome::OpenTextEntry {
                                    editing.set(true);
                                }
                            }
                        },
                    }
                }
            }

            ValueReadout {
                handle: handle.clone(),
                disabled,
                open: editing,
                testid: "knob-{param_name}",
            }

            span {
                style: format!(
                    "font-size:10px; color:var(--muted-foreground); \
                     font-weight:500; letter-spacing:0.05em; \
                     text-transform:uppercase; \
                     min-width:52px; text-align:center;"
                ),
                "{param_name}"
            }
        }
    }
}

/// Knob displaying a raw normalized value, not bound to a parameter system.
/// Useful for visualizations or custom edit handling.
#[component]
pub fn RawKnob(
    #[props(default = 0.5)] value: f64,
    #[props(default)] size: KnobSize,
    #[props(default)] label: Option<String>,
    #[props(default)] display_value: Option<String>,
    #[props(default)] color: Option<String>,
    #[props(default)] mod_min: Option<f64>,
    #[props(default)] mod_max: Option<f64>,
    #[props(default)] on_change: Option<Callback<f64>>,
    #[props(default)] disabled: bool,
) -> Element {
    let d = size.diameter();
    let val = value.clamp(0.0, 1.0);
    let accent = color.as_deref().unwrap_or(ACCENT);
    let look = KnobLook {
        diameter: d as f64,
        value: val,
        bipolar: false,
        mod_range: match (mod_min, mod_max) {
            (Some(lo), Some(hi)) => Some((lo, hi)),
            _ => None,
        },
        active: false,
        accent: crate::paint::color(accent),
        track: crate::paint::color(BORDER),
        pointer: crate::paint::color(TEXT),
        detent: crate::paint::color(TEXT_DIM),
        mod_color: crate::paint::color(SIGNAL_MOD),
        cap_fill: crate::paint::color(SURFACE),
        cap_stroke: crate::paint::color(BORDER),
    };
    let opacity = if disabled { "0.5" } else { "1.0" };
    let cursor = if disabled { "not-allowed" } else { "pointer" };

    rsx! {
        div {
            style: format!(
                "display:inline-flex; flex-direction:column; align-items:center; gap:4px; \
                 opacity:{opacity}; cursor:{cursor}; position:relative;"
            ),

            div {
                style: "position:relative; width:{d}px; height:{d}px;",
                KnobDial { look }
                if !disabled {
                    input {
                        r#type: "range",
                        style: "position:absolute; inset:0; opacity:0; cursor:pointer;",
                        min: "0", max: "1", step: "0.005",
                        value: "{val}",
                        oninput: move |evt: FormEvent| {
                            if let Ok(v) = evt.value().parse::<f64>() {
                                if let Some(cb) = &on_change { cb.call(v.clamp(0.0, 1.0)); }
                            }
                        },
                    }
                }
            }

            if let Some(display) = &display_value {
                span {
                    style: format!("font-size:10px; color:{TEXT_DIM}; font-variant-numeric:tabular-nums;"),
                    "{display}"
                }
            }
            if let Some(label) = &label {
                span {
                    style: format!("font-size:10px; color:{TEXT_DIM}; font-weight:500;"),
                    "{label}"
                }
            }
        }
    }
}
