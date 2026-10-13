//! Dynamic and spectral EQ: the model, the ring, and the universal badge.
//!
//! The DSP for both has been in `eq-dsp` all along — `BandDynamics` carries
//! range/threshold/attack/release/auto, `dynamics::spectral` does the STFT
//! with overlap-add resynthesis, and `FtsEq::live_dyn_gain_db` reports what a
//! band is currently doing. What was missing was any way to *see* or *reach*
//! it: nothing in the graph, the inspector or the faces mentioned a dynamics
//! parameter, so 11 of the 24 per-band params were unreachable from the UI.
//!
//! This module is that surface, and it is deliberately built around a control
//! rather than around a band. [`ModKnob`] takes any [`ParamHandle`] and an
//! optional [`DynState`], which is what lets a Pultec Low Boost knob be made
//! dynamic exactly the way a parametric band's Gain knob is. The band
//! inspector is then just the richest caller of the same widget.
//!
//! Visual language (Pro-Q 4 for the ring, Vital/Serum for the badge):
//!
//! - The **range arc** sweeps from the control's static value by the dynamic
//!   range — red, because it marks the extent of what the band may do.
//! - The **live arc** fills from the static value toward where dynamics have
//!   actually pushed it this instant — yellow for dynamic, violet for
//!   spectral, so the mode is readable at a glance without a label.
//! - The **badge** is the click target that cycles Static → Dynamic →
//!   Spectral, and the persistent marker that a control is modulated at all.

use dioxus::prelude::*;
use fts_audio_ui::prelude::{Knob, KnobSize, ParamHandle};
use std::f64::consts::PI;

// Mirrors of the `fts_audio_ui::theme` tokens. Held locally because the
// prelude exports the theme *types* but not the colour constants, and a
// plugin editor must not depend on a Tailwind sheet having loaded — the
// repo rule is that layout-critical values are explicit.
const RING_RANGE: &str = "#ef5350";
const RING_LIVE_DYNAMIC: &str = "#f0c040";
const RING_LIVE_SPECTRAL: &str = "#8b5cf6";
const BADGE_IDLE: &str = "#737380";

/// The knob sweep the ring is drawn against. Identical to `RangeKnob`'s, so a
/// ring laid over a `Knob` lines up with the dial's own travel rather than
/// approximately agreeing with it.
const START_ANGLE: f64 = 135.0;
const SWEEP: f64 = 270.0;

/// What a control is doing beyond holding a static value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DynMode {
    /// A plain parameter. No ring, no badge fill.
    #[default]
    Static,
    /// Band-wide dynamics: the whole band's gain rides the detector.
    Dynamic,
    /// Per-bin dynamics: only the frequencies inside the band that cross the
    /// threshold move. Requires the linear-phase spectral path.
    Spectral,
}

impl DynMode {
    /// The mode a band is in, read from the two parameters that define it.
    ///
    /// Range is the discriminator, not a separate "enabled" flag: a band with
    /// zero range is static whatever the spectral toggle says, which is what
    /// keeps the badge honest after someone dials the range back to nothing.
    #[must_use]
    pub fn from_params(range_db: f32, spectral: bool) -> Self {
        if range_db.abs() < 0.05 {
            Self::Static
        } else if spectral {
            Self::Spectral
        } else {
            Self::Dynamic
        }
    }

    #[must_use]
    pub const fn live_colour(self) -> &'static str {
        match self {
            Self::Static => BADGE_IDLE,
            Self::Dynamic => RING_LIVE_DYNAMIC,
            Self::Spectral => RING_LIVE_SPECTRAL,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Static => "Static",
            Self::Dynamic => "Dynamic",
            Self::Spectral => "Spectral",
        }
    }

    /// One click of the badge. Wraps, so the badge alone can reach every mode.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Static => Self::Dynamic,
            Self::Dynamic => Self::Spectral,
            Self::Spectral => Self::Static,
        }
    }
}

/// Everything [`ModKnob`] needs to draw and drive a control's dynamics.
///
/// Cloned into components, so the handles are the shared `ParamHandle`s rather
/// than borrowed params.
#[derive(Clone, PartialEq)]
pub struct DynState {
    /// Signed dynamic range in dB, −30..30. Zero means static.
    pub range: ParamHandle,
    /// The spectral toggle, 0/1.
    pub spectral: ParamHandle,
    /// 3 dB/oct tilt on the trigger spectrum. Pro-Q defaults this on for new
    /// spectral bands, and so does [`set_mode`].
    pub tilt: ParamHandle,
    /// Where dynamics have pushed this control right now, in dB. Published by
    /// the audio thread; `0.0` when the band is idle or static.
    pub live_db: f32,
    /// Full scale for the ring, in dB — the range parameter's own maximum.
    pub range_max_db: f32,
}

impl DynState {
    #[must_use]
    pub fn mode(&self) -> DynMode {
        DynMode::from_params(
            self.range.normalized().mul_add(2.0, -1.0) * self.range_max_db,
            self.spectral.normalized() > 0.5,
        )
    }

    /// Signed dynamic range in dB, decoded from the handle's 0..1 position.
    #[must_use]
    pub fn range_db(&self) -> f32 {
        self.range.normalized().mul_add(2.0, -1.0) * self.range_max_db
    }

    /// Apply a mode as a user gesture.
    ///
    /// Going dynamic from static needs a range to be audible at all, so this
    /// seeds −6 dB (downward compression, the overwhelmingly common case)
    /// rather than leaving the band silent-but-flagged. Going spectral also
    /// arms the tilt, matching Pro-Q's default for new spectral bands.
    /// Returning to static zeroes the range, which is what makes `mode()`
    /// agree on the next frame.
    pub fn set_mode(&self, mode: DynMode) {
        match mode {
            DynMode::Static => {
                self.range.set_as_gesture(0.5);
                self.spectral.set_as_gesture(0.0);
            }
            DynMode::Dynamic => {
                if self.range_db().abs() < 0.05 {
                    let seeded = (-6.0 / self.range_max_db).mul_add(0.5, 0.5);
                    self.range.set_as_gesture(seeded);
                }
                self.spectral.set_as_gesture(0.0);
            }
            DynMode::Spectral => {
                if self.range_db().abs() < 0.05 {
                    let seeded = (-6.0 / self.range_max_db).mul_add(0.5, 0.5);
                    self.range.set_as_gesture(seeded);
                }
                self.spectral.set_as_gesture(1.0);
                self.tilt.set_as_gesture(1.0);
            }
        }
    }
}

fn arc_point(cx: f64, cy: f64, r: f64, deg: f64) -> (f64, f64) {
    let rad = deg * PI / 180.0;
    (r.mul_add(rad.cos(), cx), r.mul_add(rad.sin(), cy))
}

/// An SVG arc between two sweep positions, both 0..1 of the dial's travel.
fn arc_path(cx: f64, cy: f64, r: f64, from: f64, to: f64) -> String {
    let a0 = SWEEP.mul_add(from.clamp(0.0, 1.0), START_ANGLE);
    let a1 = SWEEP.mul_add(to.clamp(0.0, 1.0), START_ANGLE);
    let (lo, hi) = if a0 <= a1 { (a0, a1) } else { (a1, a0) };
    let (x0, y0) = arc_point(cx, cy, r, lo);
    let (x1, y1) = arc_point(cx, cy, r, hi);
    let large = i32::from((hi - lo).abs() > 180.0);
    format!("M {x0:.2} {y0:.2} A {r:.2} {r:.2} 0 {large} 1 {x1:.2} {y1:.2}")
}

/// The ring drawn around a knob: how far dynamics may move it, and how far
/// they are moving it right now.
///
/// `base` and the two extents are all fractions of the dial's own sweep, so
/// this composes with any control regardless of its unit — which is the whole
/// point of being able to put it on a Pultec knob.
#[component]
pub fn DynRing(
    /// The control's static position, 0..1 of dial travel.
    base: f64,
    /// Signed dynamic range as a fraction of dial travel.
    range: f64,
    /// Current dynamic offset as a fraction of dial travel.
    live: f64,
    mode: DynMode,
    #[props(default = 44)] diameter: u32,
) -> Element {
    if mode == DynMode::Static {
        return rsx! {};
    }

    let d = f64::from(diameter);
    let (cx, cy) = (d / 2.0, d / 2.0);
    // Outside the dial itself, so the ring reads as an annotation on the
    // control rather than as part of its face.
    let r = d / 2.0 - 2.0;

    let range_end = (base + range).clamp(0.0, 1.0);
    let live_end = (base + live).clamp(0.0, 1.0);

    let range_arc = arc_path(cx, cy, r, base, range_end);
    let live_arc = arc_path(cx, cy, r, base, live_end);
    let live_colour = mode.live_colour();
    let show_live = (live_end - base).abs() > 0.001;

    rsx! {
        svg {
            width: "{diameter}",
            height: "{diameter}",
            view_box: "0 0 {d} {d}",
            style: "position:absolute; left:50%; top:0; transform:translateX(-50%); pointer-events:none;",
            // The permitted extent.
            path {
                d: "{range_arc}",
                fill: "none",
                stroke: "{RING_RANGE}",
                stroke_width: "2.5",
                stroke_linecap: "round",
                opacity: "0.45",
            }
            // What is actually happening, drawn over it.
            if show_live {
                path {
                    d: "{live_arc}",
                    fill: "none",
                    stroke: "{live_colour}",
                    stroke_width: "2.5",
                    stroke_linecap: "round",
                }
            }
        }
    }
}

/// The universal "this control is modulated" marker.
///
/// Serum and Vital both solve the same problem this way: a small persistent
/// mark on the control itself, coloured by what is driving it, that is also
/// the click target for changing it. Putting it on [`ModKnob`] rather than on
/// the band inspector is what makes "make this Pultec knob spectral" the same
/// gesture as making a parametric band spectral.
#[component]
pub fn DynBadge(mode: DynMode, on_cycle: EventHandler<DynMode>) -> Element {
    let colour = mode.live_colour();
    let filled = mode != DynMode::Static;
    let bg = if filled { colour } else { "transparent" };
    let title = format!("{} — click to change", mode.label());

    rsx! {
        div {
            style: format!(
                "position:absolute; right:2px; top:2px; width:10px; height:10px; \
                 border-radius:5px; border:1.5px solid {colour}; background:{bg}; \
                 cursor:pointer; z-index:2;"
            ),
            title: "{title}",
            onclick: move |e| {
                e.stop_propagation();
                on_cycle.call(mode.next());
            },
            // Spectral gets an inner void so the two active modes are
            // distinguishable without relying on hue alone.
            if mode == DynMode::Spectral {
                div {
                    style: "position:absolute; left:2.5px; top:2.5px; width:5px; height:5px; \
                            border-radius:2.5px; background:#0c0c0f;",
                }
            }
        }
    }
}

/// A labelled knob that can carry dynamics.
///
/// With `dynamics: None` this is exactly the inspector knob it replaces. With
/// `Some(state)` it gains the ring and the badge, and nothing else about the
/// control changes — which is the property that lets every face adopt it
/// incrementally.
#[component]
pub fn ModKnob(
    label: String,
    value: String,
    handle: ParamHandle,
    #[props(default)] dynamics: Option<DynState>,
) -> Element {
    let mode = dynamics.as_ref().map_or(DynMode::Static, DynState::mode);
    let base = f64::from(handle.normalized());

    // dB → dial travel. The ring shares the knob's sweep, so a range is only
    // meaningful as a fraction of the control's own span.
    let (range_frac, live_frac) = dynamics.as_ref().map_or((0.0, 0.0), |d| {
        let span = f64::from(d.range_max_db).max(1.0);
        (
            f64::from(d.range_db()) / (2.0 * span),
            f64::from(d.live_db) / (2.0 * span),
        )
    });

    let cycle = dynamics.clone();

    rsx! {
        div {
            class: "rounded-md border border-border bg-muted/20 p-2 flex flex-col items-center gap-1 min-w-0",
            style: "position:relative;",

            div { class: "text-[10px] uppercase tracking-wider text-muted-foreground", "{label}" }

            div {
                style: "position:relative; display:flex; align-items:center; justify-content:center;",
                DynRing {
                    base,
                    range: range_frac,
                    live: live_frac,
                    mode,
                }
                Knob { handle, size: KnobSize::Small }
            }

            div { class: "text-xs font-semibold tabular-nums text-foreground truncate max-w-full", "{value}" }

            if let Some(state) = cycle {
                DynBadge {
                    mode,
                    on_cycle: move |next: DynMode| state.set_mode(next),
                }
            }
        }
    }
}

/// The full per-band dynamics section, in the shape Pro-Q 4 uses.
///
/// Collapsed it is one row: the mode, the range, and a live readout. Expanded
/// it exposes what auto mode is otherwise deciding — threshold, attack,
/// release — plus the two controls that only exist for spectral bands.
///
/// Auto is the default and stays the default: Pro-Q's threshold/attack/release
/// track the incoming audio unless you take them over, and `dyn_auto` already
/// carries exactly that meaning through to `BandDynamics`.
#[component]
pub fn BandDynamicsPanel(
    state: DynState,
    threshold: ParamHandle,
    attack: ParamHandle,
    release: ParamHandle,
    auto: ParamHandle,
    density: ParamHandle,
) -> Element {
    let mut expanded = use_signal(|| false);

    let mode = state.mode();
    let range_db = state.range_db();
    let is_auto = auto.normalized() > 0.5;
    let spectral = mode == DynMode::Spectral;
    let colour = mode.live_colour();

    // The live readout is the one number that tells you the band is working.
    // Static bands show an em dash rather than a misleading 0.0.
    let live_text = if mode == DynMode::Static {
        "—".to_string()
    } else {
        format!("{:+.1} dB", state.live_db)
    };

    let state_for_mode = state.clone();
    let state_for_tilt = state.clone();
    let tilt_on = state.tilt.normalized() > 0.5;

    rsx! {
        div { class: "rounded-md border border-border bg-muted/20 p-2",

            // ── Header: mode, live readout, expander ──
            div { class: "flex items-center justify-between mb-2",
                div { class: "flex items-center gap-2",
                    div {
                        style: format!(
                            "width:8px; height:8px; border-radius:4px; background:{colour};"
                        ),
                    }
                    div { class: "text-[10px] uppercase tracking-wider text-muted-foreground", "Dynamics" }
                }
                div {
                    class: "text-xs tabular-nums",
                    style: format!("color:{colour};"),
                    "{live_text}"
                }
            }

            // ── Mode selector — the same three states the badge cycles ──
            div { class: "flex gap-1 mb-2",
                for m in [DynMode::Static, DynMode::Dynamic, DynMode::Spectral] {
                    button {
                        key: "{m.label()}",
                        class: "flex-1 text-[10px] uppercase tracking-wider rounded px-1 py-1 border border-border",
                        style: if m == mode {
                            format!("background:{}; color:#0c0c0f; font-weight:600;", m.live_colour())
                        } else {
                            "background:transparent; color:#737380;".to_string()
                        },
                        onclick: {
                            let st = state_for_mode.clone();
                            move |_| st.set_mode(m)
                        },
                        "{m.label()}"
                    }
                }
            }

            if mode != DynMode::Static {
                // ── Range: the one control that is always visible ──
                div { class: "grid grid-cols-2 gap-2 mb-2",
                    ModKnob {
                        label: "Range".to_string(),
                        value: format!("{range_db:+.1}"),
                        handle: state.range.clone(),
                    }
                    ModKnob {
                        label: if spectral { "Density".to_string() } else { "Threshold".to_string() },
                        value: if spectral {
                            format!("{:.0}%", density.normalized() * 100.0)
                        } else if is_auto {
                            "Auto".to_string()
                        } else {
                            threshold.display_value()
                        },
                        handle: if spectral { density.clone() } else { threshold.clone() },
                    }
                }

                button {
                    class: "w-full text-[10px] uppercase tracking-wider rounded px-1 py-1 border border-border text-muted-foreground",
                    onclick: move |_| {
                        let now = *expanded.read();
                        expanded.set(!now);
                    },
                    if *expanded.read() { "Less «" } else { "More »" }
                }

                if *expanded.read() {
                    div { class: "pt-2 flex flex-col gap-2",

                        // Auto governs threshold/attack/release together, so it
                        // is one switch rather than three.
                        div { class: "flex items-center justify-between",
                            div { class: "text-[10px] uppercase tracking-wider text-muted-foreground", "Auto" }
                            button {
                                class: "text-[10px] uppercase rounded px-2 py-1 border border-border",
                                style: if is_auto {
                                    "background:#f0c040; color:#0c0c0f; font-weight:600;".to_string()
                                } else {
                                    "background:transparent; color:#737380;".to_string()
                                },
                                onclick: {
                                    let a = auto.clone();
                                    move |_| a.set_as_gesture(if is_auto { 0.0 } else { 1.0 })
                                },
                                if is_auto { "On" } else { "Off" }
                            }
                        }

                        div { class: "grid grid-cols-3 gap-2",
                            ModKnob {
                                label: "Thresh".to_string(),
                                value: if is_auto { "Auto".to_string() } else { threshold.display_value() },
                                handle: threshold.clone(),
                            }
                            ModKnob {
                                label: "Attack".to_string(),
                                value: attack.display_value(),
                                handle: attack.clone(),
                            }
                            ModKnob {
                                label: "Release".to_string(),
                                value: release.display_value(),
                                handle: release.clone(),
                            }
                        }

                        // Spectral-only. The tilt is the difference between a
                        // spectral band behaving on a full mix and fighting it.
                        if spectral {
                            div { class: "flex items-center justify-between",
                                div { class: "text-[10px] uppercase tracking-wider text-muted-foreground",
                                    "Spectral Tilt (3 dB/oct)"
                                }
                                button {
                                    class: "text-[10px] uppercase rounded px-2 py-1 border border-border",
                                    style: if tilt_on {
                                        "background:#8b5cf6; color:#0c0c0f; font-weight:600;".to_string()
                                    } else {
                                        "background:transparent; color:#737380;".to_string()
                                    },
                                    onclick: {
                                        let st = state_for_tilt.clone();
                                        move |_| st.tilt.set_as_gesture(if tilt_on { 0.0 } else { 1.0 })
                                    },
                                    if tilt_on { "On" } else { "Off" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The three continuous controls a band's panel drives directly.
///
/// The popup used to move frequency/gain/Q by mutating the `EqBand` model and
/// firing `on_band_change`, which is fine for a readout but cannot host a real
/// knob — a knob needs a [`ParamHandle`] to drag against. Threading the
/// handles alongside the model lets the panel show Pro-Q's dials without
/// pushing dynamics state into `EqBand`, which describes the static curve.
#[derive(Clone, PartialEq)]
pub struct BandHandles {
    pub freq: ParamHandle,
    pub gain: ParamHandle,
    pub q: ParamHandle,
}

/// One dial in the band panel.
///
/// A thin wrapper over [`Knob`] rather than a hand-built dial. The first
/// version used `RawKnob`, which looked right and could not be dragged: it
/// fakes interaction with an invisible `<input type="range">`, a HORIZONTAL
/// slider, so a vertical drag on FREQ/GAIN/Q did nothing at all.
///
/// `Knob` is the one with real gestures — vertical drag with fine-tune, wheel,
/// double-click to reset, click-the-value to type, keyboard — and it fills the
/// dial from the centre for a bipolar parameter, which `RawKnob` cannot (it
/// hardcodes `bipolar: false`, so Gain read as a 0..1 sweep). It also takes the
/// `mod_min`/`mod_max` arc this panel needs for the dynamic range.
///
/// It renders the value under the dial rather than above it, which is not
/// Pro-Q's order. That is a deliberate trade: a dial you can actually drag and
/// type into beats one with the number in the right place.
#[component]
pub fn PanelKnob(
    label: String,
    handle: ParamHandle,
    #[props(default = "#8aa4ff".to_string())] accent: String,
    #[props(default)] dynamics: Option<DynState>,
) -> Element {
    let mode = dynamics.as_ref().map_or(DynMode::Static, DynState::mode);
    let base = f64::from(handle.normalized());

    // The dynamic range as an arc on the dial: from where the control sits to
    // where dynamics may carry it, in the dial's own geometry.
    let (mod_min, mod_max) = dynamics
        .as_ref()
        .and_then(|d| {
            if mode == DynMode::Static {
                return None;
            }
            let span = f64::from(d.range_max_db).max(1.0);
            let end = (base + f64::from(d.range_db()) / (2.0 * span)).clamp(0.0, 1.0);
            Some((base.min(end), base.max(end)))
        })
        .map_or((None, None), |(a, b)| (Some(a), Some(b)));

    // A modulated dial takes the mode's colour so the panel reads at a glance;
    // an ordinary one keeps the band's.
    let colour = if mode == DynMode::Static {
        accent
    } else {
        mode.live_colour().to_string()
    };

    rsx! {
        Knob {
            handle,
            size: KnobSize::Small,
            label,
            color: colour,
            mod_min,
            mod_max,
        }
    }
}
