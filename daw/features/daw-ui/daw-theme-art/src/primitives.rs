//! The primitives every FTS surface is built from.
//!
//! Each is a self-contained `<svg>` drawn from the palette, so the same
//! component can be
//!
//! - composed into a live web mixer ([`crate::strip::MixerStrip`]), and
//! - rasterised into the single REAPER image it corresponds to
//!   (`mcp_volbg`, `mcp_volthumb`, `mcp_solo`, …).
//!
//! That shared origin is what makes the two surfaces match. A primitive
//! that only existed in one of them would be a place they could drift.
//!
//! # Everything takes its size
//!
//! REAPER's images have sizes WALTER expects — `mcp_bg` is 4×4 — so no
//! primitive chooses its own dimensions. It draws into the box it is given
//! and stays correct from 2px to 200px, which is also what makes it usable
//! in a flex layout on the web.

use daw_theme::{Color, Theme};
use dioxus::prelude::*;

/// The states a themed control can be in.
///
/// REAPER ships a separate image per state; on the web this is one
/// component with a prop. Same drawing either way.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ControlState {
    /// Resting.
    #[default]
    Off,
    /// Engaged — the accent-lit state (solo on, mute on).
    On,
    /// Pointer over it.
    Hover,
    /// Held down.
    Pressed,
    /// Not available.
    Disabled,
}

impl ControlState {
    /// Surface for this state, given the theme.
    pub fn surface(self, t: &Theme) -> Color {
        let c = &t.chrome;
        let control = c.surface_raised.shade(0.06);
        match self {
            Self::Off => control,
            Self::On => c.surface_raised.mix(c.accent, 0.35),
            Self::Hover => control.shade(0.08),
            Self::Pressed => control.shade(-0.10),
            Self::Disabled => c.surface,
        }
    }

    /// Text/glyph colour for this state.
    pub fn ink(self, t: &Theme) -> Color {
        let c = &t.chrome;
        match self {
            Self::On => c.selected,
            Self::Disabled => c.text_faint,
            _ => c.text,
        }
    }
}

/// Common props: the box to fill.
#[derive(Props, Clone, PartialEq)]
pub struct SizeProps {
    pub width: u32,
    pub height: u32,
}

/// A raised panel — the body of a strip or a grouped section.
#[component]
pub fn Panel(props: SizeProps) -> Element {
    let t = Theme::default();
    let (w, h) = (props.width as f32, props.height as f32);
    // Radius must stay under half the shorter side or the shape inverts at
    // the tiny sizes REAPER's nine-slice sources use.
    let r = (w.min(h) / 2.0 - 0.5).clamp(0.0, t.metrics.radius);
    rsx! {
        svg {
            width: "{props.width}", height: "{props.height}",
            view_box: "0 0 {props.width} {props.height}",
            xmlns: "http://www.w3.org/2000/svg",
            rect {
                x: "0.5", y: "0.5",
                width: "{(w - 1.0).max(0.0)}", height: "{(h - 1.0).max(0.0)}",
                rx: "{r}",
                fill: "{t.chrome.surface_raised.css()}",
                stroke: "{t.chrome.border.css()}", stroke_width: "1",
            }
        }
    }
}

/// The trough a fader or scrollbar handle runs in.
#[component]
pub fn Groove(props: SizeProps) -> Element {
    let t = Theme::default();
    let (w, h) = (props.width as f32, props.height as f32);
    // Vertical unless the box is wider than tall — one primitive serves the
    // mixer fader and a horizontal pan slider.
    let vertical = h >= w;
    let thickness = if vertical { w * 0.35 } else { h * 0.35 }.max(3.0);
    let (x, y, gw, gh) = if vertical {
        ((w - thickness) / 2.0, 0.5, thickness, (h - 1.0).max(0.0))
    } else {
        (0.5, (h - thickness) / 2.0, (w - 1.0).max(0.0), thickness)
    };
    rsx! {
        svg {
            width: "{props.width}", height: "{props.height}",
            view_box: "0 0 {props.width} {props.height}",
            xmlns: "http://www.w3.org/2000/svg",
            rect {
                x: "{x}", y: "{y}", width: "{gw}", height: "{gh}",
                rx: "{thickness / 2.0}",
                fill: "{t.chrome.surface_sunken.css()}",
                stroke: "{t.chrome.border.css()}", stroke_width: "1",
            }
        }
    }
}

/// A fader cap.
#[derive(Props, Clone, PartialEq)]
pub struct ThumbProps {
    pub width: u32,
    pub height: u32,
    /// Accent to tint the cap. `None` uses the theme accent — this is the
    /// hook REAPER's per-colour `<accent>/mcp_volthumb.png` variants use.
    #[props(default)]
    pub accent: Option<Color>,
}

#[component]
pub fn Thumb(props: ThumbProps) -> Element {
    let t = Theme::default();
    let accent = props.accent.unwrap_or(t.chrome.accent);
    let (w, h) = (props.width as f32, props.height as f32);
    let r = (w.min(h) / 4.0).clamp(1.0, 4.0);
    // Ribbing: the horizontal lines that make a cap read as grippable.
    let ribs = ((h / 3.0) as u32).clamp(0, 4);
    rsx! {
        svg {
            width: "{props.width}", height: "{props.height}",
            view_box: "0 0 {props.width} {props.height}",
            xmlns: "http://www.w3.org/2000/svg",
            rect {
                x: "0.5", y: "0.5",
                width: "{(w - 1.0).max(0.0)}", height: "{(h - 1.0).max(0.0)}",
                rx: "{r}",
                fill: "{accent.css()}",
                stroke: "{accent.shade(-0.3).css()}", stroke_width: "1",
            }
            for i in 0..ribs {
                rect {
                    key: "{i}",
                    x: "{w * 0.25}",
                    y: "{h * (0.3 + 0.15 * i as f32)}",
                    width: "{w * 0.5}", height: "1",
                    fill: "{accent.shade(-0.35).css()}",
                }
            }
        }
    }
}

/// A labelled control button — mute, solo, arm, FX bypass.
#[derive(Props, Clone, PartialEq)]
pub struct ButtonProps {
    pub width: u32,
    pub height: u32,
    #[props(default)]
    pub state: ControlState,
    /// One or two characters. Longer labels are the caller's problem —
    /// REAPER's buttons are tiny and clipping is worse than not fitting.
    #[props(default)]
    pub label: Option<String>,
    /// Overrides the state surface — lets mute/solo keep their own colours
    /// when engaged rather than all lighting the same accent.
    #[props(default)]
    pub on_color: Option<Color>,
}

#[component]
pub fn Button(props: ButtonProps) -> Element {
    let t = Theme::default();
    let (w, h) = (props.width as f32, props.height as f32);
    let r = (w.min(h) / 4.0).clamp(0.0, t.metrics.radius);
    let fill = match (props.state, props.on_color) {
        (ControlState::On, Some(c)) => c,
        (s, _) => s.surface(&t),
    };
    let ink = props.state.ink(&t);
    rsx! {
        svg {
            width: "{props.width}", height: "{props.height}",
            view_box: "0 0 {props.width} {props.height}",
            xmlns: "http://www.w3.org/2000/svg",
            rect {
                x: "0.5", y: "0.5",
                width: "{(w - 1.0).max(0.0)}", height: "{(h - 1.0).max(0.0)}",
                rx: "{r}",
                fill: "{fill.css()}",
                stroke: "{t.chrome.border.css()}", stroke_width: "1",
            }
            if let Some(label) = props.label.as_ref() {
                text {
                    x: "{w / 2.0}", y: "{h / 2.0}",
                    text_anchor: "middle",
                    dominant_baseline: "central",
                    font_family: "Fira Sans, sans-serif",
                    font_size: "{(h * 0.62).clamp(6.0, 14.0)}",
                    fill: "{ink.css()}",
                    "{label}"
                }
            }
        }
    }
}

/// A level meter.
#[derive(Props, Clone, PartialEq)]
pub struct MeterProps {
    pub width: u32,
    pub height: u32,
    /// Level, 0–1.
    #[props(default = 0.0)]
    pub level: f32,
    /// Distinguishes this meter's gradient from another's on the same
    /// page.
    ///
    /// SVG ids are document-global. Two meters both defining `id="meter"`
    /// means every `url(#meter)` resolves to whichever came first — and
    /// since the ramp is `userSpaceOnUse`, keyed to *that* meter's height,
    /// a second meter of a different height silently borrows the first
    /// one's zones. A stereo strip is two meters side by side, so this is
    /// not hypothetical.
    #[props(default)]
    pub tag: String,
    /// Decaying peak-hold, 0–1. `None` draws no hold mark at all.
    ///
    /// Optional rather than defaulted to the level, because "no hold" and
    /// "hold sitting exactly on the level" are different pictures, and
    /// every drawing that existed before peak-hold did means the first.
    #[props(default)]
    pub hold: Option<f32>,
}

#[component]
pub fn Meter(props: MeterProps) -> Element {
    let t = Theme::default();
    let s = &t.signal;
    let (w, h) = (props.width as f32, props.height as f32);
    let level = props.level.clamp(0.0, 1.0);
    let lit = h * level;
    let id = format!("meter{}", props.tag);

    // The ramp is a gradient rather than three blocks so the transition
    // reads as continuous — a metered signal doesn't step between zones.
    rsx! {
        svg {
            width: "{props.width}", height: "{props.height}",
            view_box: "0 0 {props.width} {props.height}",
            xmlns: "http://www.w3.org/2000/svg",
            defs {
                // userSpaceOnUse over the FULL meter height, not the lit
                // rect. With the default objectBoundingBox the ramp is
                // squeezed into whatever is currently lit, so a signal at
                // 25% renders green→yellow→red and looks like it is
                // clipping. The zone a level falls in must depend on the
                // level, not on the size of the bar drawn for it.
                linearGradient {
                    id: "{id}",
                    gradient_units: "userSpaceOnUse",
                    x1: "0", y1: "{h}", x2: "0", y2: "0",
                    stop { offset: "0",    stop_color: "{s.meter_safe.css()}" }
                    stop { offset: "0.75", stop_color: "{s.meter_warn.css()}" }
                    stop { offset: "1",    stop_color: "{s.meter_danger.css()}" }
                }
            }
            rect {
                x: "0", y: "0", width: "{w}", height: "{h}",
                fill: "{t.chrome.surface_sunken.css()}",
            }
            if lit > 0.0 {
                rect {
                    x: "0", y: "{h - lit}", width: "{w}", height: "{lit}",
                    fill: "url(#{id})",
                }
            }
            // The hold mark, in the colour of the zone it is *in* rather
            // than a fixed white line: a hold sitting in the red has to
            // read as a peak that clipped, and a white tick above a green
            // bar reads as a scale mark instead.
            if let Some(hold) = props.hold {
                {
                    let hold = hold.clamp(0.0, 1.0);
                    let y = (h - h * hold - 1.0).max(0.0);
                    rsx! {
                        rect {
                            x: "0", y: "{y}", width: "{w}", height: "1",
                            fill: "url(#{id})",
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::render_svg;

    #[test]
    fn every_primitive_survives_the_sizes_reaper_uses() {
        // 4x4 and 2x2 are real nine-slice source sizes; 1x1 is where inset
        // arithmetic goes negative and yields SVG resvg rejects.
        for (w, h) in [(1, 1), (2, 2), (4, 4), (16, 60), (23, 55), (120, 300)] {
            for (name, svg) in [
                (
                    "Panel",
                    render_svg(
                        Panel,
                        SizeProps {
                            width: w,
                            height: h,
                        },
                    ),
                ),
                (
                    "Groove",
                    render_svg(
                        Groove,
                        SizeProps {
                            width: w,
                            height: h,
                        },
                    ),
                ),
            ] {
                assert!(svg.contains("<svg"), "{name} at {w}x{h}: {svg}");
                let opts = resvg::usvg::Options::default();
                assert!(
                    resvg::usvg::Tree::from_str(&svg, &opts).is_ok(),
                    "{name} at {w}x{h} produced SVG resvg rejects: {svg}"
                );
            }
        }
    }

    #[test]
    fn groove_turns_horizontal_when_the_box_is_wide() {
        // One primitive has to serve a vertical fader and a horizontal pan
        // slider, or the two drift apart.
        let tall = render_svg(
            Groove,
            SizeProps {
                width: 16,
                height: 60,
            },
        );
        let wide = render_svg(
            Groove,
            SizeProps {
                width: 60,
                height: 16,
            },
        );
        assert_ne!(tall, wide, "groove ignored its aspect");
    }

    #[test]
    fn button_states_are_visually_distinct() {
        // If two states render identically the control stops communicating.
        let mut seen = Vec::new();
        for state in [
            ControlState::Off,
            ControlState::On,
            ControlState::Hover,
            ControlState::Pressed,
            ControlState::Disabled,
        ] {
            let svg = render_svg(
                Button,
                ButtonProps {
                    width: 20,
                    height: 14,
                    state,
                    label: Some("M".into()),
                    on_color: None,
                },
            );
            assert!(!seen.contains(&svg), "state {state:?} duplicates another");
            seen.push(svg);
        }
    }

    #[test]
    fn an_engaged_button_can_keep_its_own_colour() {
        // Mute and solo must not both light up the accent.
        let t = Theme::default();
        let svg = render_svg(
            Button,
            ButtonProps {
                width: 20,
                height: 14,
                state: ControlState::On,
                label: None,
                on_color: Some(t.signal.mute),
            },
        );
        assert!(svg.contains(&t.signal.mute.to_hex()), "{svg}");
    }

    #[test]
    fn the_meter_ramp_spans_the_scale_not_the_lit_bar() {
        // Regression: with an objectBoundingBox gradient the ramp is
        // squeezed into the lit rect, so a quiet signal renders red at its
        // top and reads as clipping.
        let svg = render_svg(
            Meter,
            MeterProps {
                tag: String::new(),
                hold: None,
                width: 6,
                height: 60,
                level: 0.25,
            },
        );
        assert!(
            svg.contains("userSpaceOnUse"),
            "meter ramp is relative to the lit bar: {svg}"
        );
    }

    #[test]
    fn meter_level_changes_what_is_drawn() {
        let empty = render_svg(
            Meter,
            MeterProps {
                tag: String::new(),
                hold: None,
                width: 6,
                height: 60,
                level: 0.0,
            },
        );
        let half = render_svg(
            Meter,
            MeterProps {
                tag: String::new(),
                hold: None,
                width: 6,
                height: 60,
                level: 0.5,
            },
        );
        let full = render_svg(
            Meter,
            MeterProps {
                tag: String::new(),
                hold: None,
                width: 6,
                height: 60,
                level: 1.0,
            },
        );
        assert_ne!(empty, half);
        assert_ne!(half, full);
        // An empty meter draws no lit bar at all rather than a zero-height
        // rect, which some rasterisers turn into a hairline.
        assert_eq!(empty.matches("<rect").count(), 1, "{empty}");
    }

    #[test]
    fn meter_level_clamps_rather_than_overflowing() {
        for level in [-5.0, 5.0, f32::NAN] {
            let svg = render_svg(
                Meter,
                MeterProps {
                    tag: String::new(),
                    hold: None,
                    width: 6,
                    height: 60,
                    level,
                },
            );
            let opts = resvg::usvg::Options::default();
            assert!(
                resvg::usvg::Tree::from_str(&svg, &opts).is_ok(),
                "level {level} produced invalid SVG: {svg}"
            );
        }
    }

    #[test]
    fn thumb_takes_an_accent_so_reapers_colour_variants_work() {
        let custom = Color::rgb(0xd1, 0x28, 0x3c);
        let svg = render_svg(
            Thumb,
            ThumbProps {
                width: 20,
                height: 12,
                accent: Some(custom),
            },
        );
        assert!(svg.contains(&custom.to_hex()), "{svg}");
    }

    #[test]
    fn primitives_draw_from_the_palette() {
        let t = Theme::default();
        let svg = render_svg(
            Panel,
            SizeProps {
                width: 40,
                height: 40,
            },
        );
        assert!(svg.contains(&t.chrome.surface_raised.to_hex()), "{svg}");
    }
}
