//! Panel buttons and indicators — the console's switching.
//!
//! A rack unit's switch is a bat toggle; a console's is a latching rectangular
//! button with a lamp under it, and the lamp is how you read the channel from
//! two feet away. Same job, different idiom, so it is a different widget.
//!
//! Buttons come in two states of wiring, and both draw: one bound to a
//! parameter, and one that is only on the panel because the unit has it. The
//! second is deliberate — the panel is the specification for what the DSP
//! still owes, and drawing it is how that stays visible.

use dioxus::prelude::*;

use crate::hardware::button_kit::{ButtonSpec, Lit};
use crate::param::ParamHandle;

/// Which button a panel is asking for.
///
/// The *shape* is what tells an 1176's ratio bank from an SSL's illuminated
/// square — not the colour, which is why a colour prop alone could never make
/// these read as different parts.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ButtonStyle {
    /// A latching rectangular cap with a jewel under it. The desk idiom.
    #[default]
    Console,
    /// The cap itself lights and the legend glows through it, in a surround.
    Illuminated,
    /// No lamp: read by which one is down. The 1176's ratio bank.
    PushIn,
    /// A small hard-cornered backlit switch, for rows of them.
    Square,
    /// A machined metal cap with a jewel under it.
    Metal,
}

impl ButtonStyle {
    /// Every button in the kit, so a test or a contact sheet can walk them
    /// all without anyone having to remember to add the new one.
    pub const ALL: [ButtonStyle; 5] = [
        Self::Console,
        Self::Illuminated,
        Self::PushIn,
        Self::Square,
        Self::Metal,
    ];

    /// What this button is made of — see
    /// [`button_kit`](crate::hardware::button_kit).
    pub fn spec(self) -> &'static ButtonSpec {
        use crate::hardware::button_parts as kit;
        match self {
            Self::Console => &kit::CONSOLE,
            Self::Illuminated => &kit::ILLUMINATED,
            Self::PushIn => &kit::PUSH_IN,
            Self::Square => &kit::SQUARE,
            Self::Metal => &kit::METAL,
        }
    }
}

/// A latching panel button with an indicator lamp beneath it.
///
/// With `handle`, it toggles a parameter and the lamp follows it. Without one,
/// it draws in its off state and does nothing — see the module docs.
#[component]
pub fn PanelButton(
    #[props(default)] handle: Option<ParamHandle>,
    testid: String,
    scale: f64,
    label: String,
    /// Face colour.
    #[props(default = "#e6e2d4".to_string())]
    color: String,
    /// Label colour.
    #[props(default = "#23252a".to_string())]
    ink: String,
    /// Lamp colour. Empty draws no lamp, whatever the style would do.
    #[props(default = "#43d17a".to_string())]
    led: String,
    /// Which part this is. Defaults to the desk idiom.
    #[props(default)]
    style: ButtonStyle,
    #[props(default = 34.0)] w: f64,
    #[props(default = 46.0)] h: f64,
) -> Element {
    let on = handle
        .as_ref()
        .map(|h| h.normalized() >= 0.5)
        .unwrap_or(false);
    let wired = handle.is_some();
    let spec = style.spec();
    // An empty `led` overrides the part: a panel can ask for an unlit one of
    // anything, which is how a face says "this switch has no lamp on ours".
    let lit = if led.is_empty() { Lit::Unlit } else { spec.lit };

    let cap_w = w * scale;
    let cap_h = h * scale;

    // A backlit cap *is* the lamp, so when it is on the cap takes the lamp's
    // colour rather than its own. Off, it falls back to the face colour —
    // an unlit switch is still a coloured cap, not a hole.
    let cap_color = match (lit, on) {
        (Lit::Backlit { .. }, true) => led.clone(),
        _ => color.clone(),
    };
    let glow = match (lit, on) {
        (Lit::Backlit { bloom }, true) => {
            format!(", 0 0 {:.1}px {led}", bloom * scale)
        }
        _ => String::new(),
    };

    let cap = rsx! {
        div {
            "data-testid": "hw-button-{testid}-cap",
            style: format!(
                "width:{cap_w:.1}px; height:{cap_h:.1}px; border-radius:{:.1}px; \
                 display:flex; align-items:center; justify-content:center; \
                 text-align:center; line-height:1.05; \
                 font-size:{:.1}px; font-weight:800; letter-spacing:0.02em; \
                 color:{ink}; cursor:{}; \
                 background:{}; \
                 border:{:.1}px solid {}; \
                 box-shadow:{}{glow};",
                spec.cap.radius * scale,
                spec.legend * scale,
                if wired { "pointer" } else { "default" },
                spec.cap.finish.css(&cap_color),
                (1.0 * scale).max(1.0),
                spec.cap.border,
                spec.shadow(on, scale),
            ),
            onclick: {
                let handle = handle.clone();
                move |_| {
                    if let Some(handle) = &handle {
                        handle.begin_edit();
                        handle.set_normalized(if on { 0.0 } else { 1.0 });
                        handle.end_edit();
                    }
                }
            },
            "{label}"
        }
    };

    rsx! {
        div {
            "data-testid": "hw-button-{testid}",
            "data-on": "{on}",
            "data-wired": "{wired}",
            "data-style": "{style:?}",
            style: format!(
                "display:flex; flex-direction:column; align-items:center; gap:{:.1}px;",
                match lit {
                    Lit::Jewel { gap, .. } => gap * scale,
                    _ => 0.0,
                },
            ),

            // The surround, where the part has one: it stops a backlit cap's
            // glow bleeding into the panel, and gives the cap somewhere to
            // sink into.
            if let Some(s) = spec.surround {
                div {
                    "data-testid": "hw-button-{testid}-surround",
                    style: format!(
                        "padding:{:.1}px; border-radius:{:.1}px; background:{}; \
                         box-shadow:inset 0 {:.1}px {:.1}px rgba(0,0,0,0.55); \
                         line-height:0;",
                        s.pad * scale,
                        s.radius * scale,
                        s.color,
                        (1.0 * scale).max(1.0),
                        3.0 * scale,
                    ),
                    {cap.clone()}
                }
            } else {
                {cap.clone()}
            }

            // The jewel below, on the parts that have one.
            if let Lit::Jewel { d, .. } = lit {
                Lamp { scale, color: led.clone(), lit: on, d }
            }
        }
    }
}

/// A panel indicator lamp — a domed jewel in a bezel.
///
/// Built from explicit stops rather than `color-mix` on the lamp's colour: the
/// core has to be *brighter than the colour itself* for the lens to read as
/// lit from within, and a mix that silently fails leaves a dark disc that
/// looks like a sticker. The colour supplies the hue in the middle band;
/// white and black do the lighting.
#[component]
pub fn Lamp(
    scale: f64,
    color: String,
    /// Lit lamps glow; dark ones still show as an unlit lens, because an empty
    /// hole on a panel reads as a missing part.
    #[props(default = true)]
    lit: bool,
    #[props(default = 9.0)] d: f64,
) -> Element {
    rsx! {
        div {
            "data-testid": "hw-lamp",
            "data-lit": "{lit}",
            style: format!(
                "width:{:.1}px; height:{:.1}px; border-radius:50%; \
                 background:{}; box-shadow:{}; \
                 border:{:.1}px solid rgba(0,0,0,0.65);",
                d * scale,
                d * scale,
                if lit {
                    format!(
                        "radial-gradient(circle at 36% 30%, \
                           rgba(255,255,255,0.92) 0%, \
                           rgba(255,255,255,0.35) 16%, \
                           {color} 46%, \
                           {color} 66%, \
                           rgba(0,0,0,0.55) 100%)"
                    )
                } else {
                    format!(
                        "radial-gradient(circle at 36% 30%, \
                           rgba(255,255,255,0.10) 0%, \
                           {color} 52%, \
                           rgba(0,0,0,0.72) 100%)"
                    )
                },
                if lit {
                    format!(
                        "0 0 {:.1}px {}, inset 0 {:.1}px {:.1}px rgba(0,0,0,0.35)",
                        d * 0.7 * scale,
                        color,
                        d * 0.16 * scale,
                        d * 0.26 * scale,
                    )
                } else {
                    format!("inset 0 0 {:.1}px rgba(0,0,0,0.75)", d * 0.34 * scale)
                },
                (1.0 * scale).max(1.0),
            ),
        }
    }
}

/// Segment boundaries of a console output meter, in dB — the ladder the
/// numbers are printed against.
pub const METER_STEPS_DB: &[f64] = &[
    0.0, -2.0, -4.0, -6.0, -8.0, -10.0, -15.0, -20.0, -30.0, -40.0,
];

/// Which segments are lit at a given level, top segment first.
///
/// A segment lights when the level reaches its bottom edge, which is what
/// makes a ladder read as a bar rather than as a dot.
pub fn lit_segments(level_db: f32) -> usize {
    METER_STEPS_DB
        .iter()
        .rev()
        .take_while(|edge| (level_db as f64) >= **edge)
        .count()
}

/// A segmented LED level meter.
#[component]
pub fn LedMeter(
    scale: f64,
    level_db: f32,
    #[props(default = 130.0)] h: f64,
    #[props(default = 13.0)] w: f64,
) -> Element {
    let lit = lit_segments(level_db);
    let count = METER_STEPS_DB.len();
    let seg_h = h / count as f64;

    rsx! {
        div {
            "data-testid": "hw-led-meter",
            "data-lit": "{lit}",
            style: format!(
                "display:flex; flex-direction:column-reverse; gap:{:.1}px; \
                 width:{:.1}px; height:{:.1}px; padding:{:.1}px; \
                 background:#0a0b0c; border-radius:{:.1}px; \
                 border:{:.1}px solid rgba(0,0,0,0.7);",
                1.0 * scale,
                w * scale,
                h * scale,
                1.5 * scale,
                2.0 * scale,
                (1.0 * scale).max(1.0),
            ),

            for index in 0..count {
                {
                    // Top of the ladder is the hot end: red, then amber, then
                    // green, as every console meter has ever been.
                    let from_top = count - 1 - index;
                    let hue = if from_top == 0 {
                        "#e0483a"
                    } else if from_top <= 2 {
                        "#e0a03a"
                    } else {
                        "#5ad24a"
                    };
                    let on = index < lit;
                    rsx! {
                        div {
                            style: format!(
                                "flex:1; min-height:{:.1}px; border-radius:{:.1}px; background:{};",
                                (seg_h - 1.0).max(1.0) * scale,
                                1.0 * scale,
                                if on {
                                    hue.to_string()
                                } else {
                                    format!("color-mix(in oklab, {hue} 16%, #0e1012)")
                                },
                            ),
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

    #[test]
    fn silence_lights_nothing_and_zero_lights_everything() {
        assert_eq!(lit_segments(-90.0), 0);
        assert_eq!(lit_segments(0.0), METER_STEPS_DB.len());
    }

    #[test]
    fn the_ladder_fills_from_the_bottom() {
        let quiet = lit_segments(-35.0);
        let loud = lit_segments(-6.0);
        assert!(quiet > 0 && quiet < loud, "{quiet} then {loud}");
        assert!(loud < METER_STEPS_DB.len());
    }

    #[test]
    fn a_segment_lights_at_its_own_edge() {
        // -20 is a printed edge: reaching it lights that segment, and a hair
        // under it does not.
        assert!(lit_segments(-20.0) > lit_segments(-20.1));
    }

    #[test]
    fn the_scale_runs_downward_without_repeating() {
        assert!(METER_STEPS_DB.windows(2).all(|w| w[0] > w[1]));
    }
}

/// A horizontal LED ladder with its scale printed above it — the Distressor's
/// gain-reduction display.
///
/// Reads right to left: 1 dB at the right end, the deepest reduction at the
/// left, so the row fills leftward as the compressor works. Green through
/// yellow to red, and the numbers are the dB each lamp stands for.
#[component]
pub fn LedBar(
    scale: f64,
    /// dB the row is showing (positive = reduction).
    value_db: f32,
    /// The printed scale, left (deepest) to right (least).
    steps: Vec<f64>,
    #[props(default = 9.0)] led_d: f64,
    #[props(default = 20.0)] pitch: f64,
    #[props(default = "#cfd4d8".to_string())] ink: String,
) -> Element {
    rsx! {
        div {
            "data-testid": "hw-led-bar",
            // Fixed-width cells rather than gaps: a row has to be exactly as
            // wide as `labels * pitch`, or the panel cannot place it.
            style: "display:flex; align-items:flex-end;",
            for step in steps.iter().copied() {
                {
                    // A lamp lights once the reduction reaches the number
                    // printed above it.
                    let lit = (value_db as f64) >= step;
                    let hue = if step >= 12.0 {
                        "#e0483a"
                    } else if step >= 6.0 {
                        "#e8c53a"
                    } else {
                        "#5ad24a"
                    };
                    rsx! {
                        div {
                            style: format!(
                                "width:{:.1}px; display:flex; flex-direction:column; \
                                 align-items:center; gap:{:.1}px;",
                                pitch * scale,
                                3.0 * scale,
                            ),
                            div {
                                style: format!(
                                    "font-size:{:.1}px; font-weight:700; color:{ink};",
                                    7.0 * scale,
                                ),
                                "{step:.0}"
                            }
                            Lamp { scale, color: hue.to_string(), lit, d: led_d }
                        }
                    }
                }
            }
        }
    }
}

/// A row of labelled LEDs that selects a stepped parameter — the Distressor's
/// ratio row, where the lamp *is* the readout and the button beside it steps
/// through them.
#[component]
pub fn LedSelect(
    handle: ParamHandle,
    testid: String,
    scale: f64,
    labels: Vec<String>,
    #[props(default = 9.0)] led_d: f64,
    #[props(default = 44.0)] pitch: f64,
    #[props(default = "#cfd4d8".to_string())] ink: String,
    /// Colour of the lit lamp, per position. Falls back to green.
    #[props(default)]
    colors: Vec<String>,
) -> Element {
    let count = labels.len().max(1);
    let selected = if count > 1 {
        (handle.normalized().clamp(0.0, 1.0) * (count - 1) as f32).round() as usize
    } else {
        0
    };

    rsx! {
        div {
            "data-testid": "hw-led-select-{testid}",
            "data-index": "{selected}",
            style: "display:flex; align-items:flex-end;",
            for (index , label) in labels.iter().enumerate() {
                {
                    let active = index == selected;
                    let color = colors
                        .get(index)
                        .cloned()
                        .unwrap_or_else(|| "#5ad24a".to_string());
                    let handle = handle.clone();
                    let step = if count > 1 {
                        index as f32 / (count - 1) as f32
                    } else {
                        0.0
                    };
                    rsx! {
                        div {
                            "data-testid": "hw-led-select-{testid}-{index}",
                            style: format!(
                                "width:{:.1}px; display:flex; flex-direction:column; \
                                 align-items:center; gap:{:.1}px; cursor:pointer;",
                                pitch * scale,
                                3.0 * scale,
                            ),
                            onclick: move |_| {
                                handle.begin_edit();
                                handle.set_normalized(step);
                                handle.end_edit();
                            },
                            div {
                                style: format!(
                                    "font-size:{:.1}px; font-weight:700; color:{ink}; \
                                     opacity:{};",
                                    7.5 * scale,
                                    if active { "1" } else { "0.66" },
                                ),
                                "{label}"
                            }
                            Lamp { scale, color, lit: active, d: led_d }
                        }
                    }
                }
            }
        }
    }
}
