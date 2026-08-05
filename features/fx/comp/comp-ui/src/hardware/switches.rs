//! Panel switches — the physical toggle and the 1176's ratio buttons.
//!
//! Both drive a stepped [`ParamHandle`] (built by
//! [`crate::profile_handle`]), so a click sets a detent position rather than
//! writing a value: the handle owns the mapping from "the 8 button" to
//! ratio 8:1.

use audiocore_core::prelude::*;
use fts_ui_audio::prelude::*;

/// Normalized position of detent `index` out of `count`.
fn detent(index: usize, count: usize) -> f32 {
    if count < 2 {
        0.0
    } else {
        index as f32 / (count - 1) as f32
    }
}

/// Which detent a normalized position is nearest.
fn detent_index(normalized: f32, count: usize) -> usize {
    if count < 2 {
        return 0;
    }
    (normalized.clamp(0.0, 1.0) * (count - 1) as f32).round() as usize
}

/// A two-position panel toggle — LA-2A COMPRESS/LIMIT, a POWER switch.
///
/// Drawn as a bat handle that leans to the selected side, because that is the
/// only thing about a toggle switch anyone reads.
#[component]
pub fn ToggleSwitch(
    handle: ParamHandle,
    testid: String,
    scale: f64,
    /// Legends for the two positions, top then bottom.
    labels: [String; 2],
    #[props(default = "#2b2620".to_string())] ink: String,
) -> Element {
    let normalized = handle.normalized();
    let index = detent_index(normalized, 2);

    let body_w = 22.0 * scale;
    let body_h = 44.0 * scale;
    let handle_h = body_h * 0.46;
    // The bat leans toward the selected legend.
    let handle_top = if index == 0 { body_h * 0.06 } else { body_h * 0.48 };

    rsx! {
        div {
            "data-testid": "hw-switch-{testid}",
            "data-index": "{index}",
            style: format!(
                "display:flex; align-items:center; gap:{:.1}px;",
                6.0 * scale,
            ),

            div {
                style: format!(
                    "display:flex; flex-direction:column; justify-content:space-between; \
                     height:{body_h:.1}px; font-size:{:.1}px; font-weight:700; \
                     letter-spacing:{:.2}px; text-transform:uppercase; color:{ink};",
                    8.5 * scale,
                    1.0 * scale,
                ),
                span { style: if index == 0 { "opacity:1;" } else { "opacity:0.45;" }, "{labels[0]}" }
                span { style: if index == 1 { "opacity:1;" } else { "opacity:0.45;" }, "{labels[1]}" }
            }

            div {
                style: format!(
                    "position:relative; width:{body_w:.1}px; height:{body_h:.1}px; \
                     border-radius:{:.1}px; cursor:pointer; \
                     background:linear-gradient(180deg, #17171a, #0a0a0c); \
                     border:{:.1}px solid rgba(0,0,0,0.6); \
                     box-shadow:inset 0 0 {:.1}px rgba(0,0,0,0.7);",
                    5.0 * scale,
                    (1.0 * scale).max(1.0),
                    5.0 * scale,
                ),
                onclick: {
                    let handle = handle.clone();
                    move |_| {
                        let next = detent(1 - index, 2);
                        handle.begin_edit();
                        handle.set_normalized(next);
                        handle.end_edit();
                    }
                },
                // The bat handle.
                div {
                    style: format!(
                        "position:absolute; left:{:.1}px; top:{handle_top:.1}px; \
                         width:{:.1}px; height:{handle_h:.1}px; border-radius:{:.1}px; \
                         background:linear-gradient(180deg, #e6e4de, #9c9a94);",
                        body_w * 0.28,
                        body_w * 0.44,
                        body_w * 0.22,
                    ),
                }
            }
        }
    }
}

/// The 1176's vertical ratio buttons.
///
/// Radio-like: exactly one is in at a time, and the one that is in stays in.
/// (The real unit lets you push several at once — "all buttons" — which the
/// profile models as one more detent rather than as real multi-select.)
#[component]
pub fn RatioButtons(
    handle: ParamHandle,
    testid: String,
    scale: f64,
    labels: Vec<String>,
    #[props(default = "#e8e2d8".to_string())] ink: String,
) -> Element {
    let count = labels.len();
    let selected = detent_index(handle.normalized(), count);

    let bw = 34.0 * scale;
    let bh = 22.0 * scale;

    rsx! {
        div {
            "data-testid": "hw-buttons-{testid}",
            "data-index": "{selected}",
            style: format!("display:flex; flex-direction:column; gap:{:.1}px;", 4.0 * scale),

            for (i , label) in labels.iter().enumerate() {
                div {
                    "data-testid": "hw-button-{testid}-{i}",
                    style: format!(
                        "width:{bw:.1}px; height:{bh:.1}px; border-radius:{:.1}px; \
                         display:flex; align-items:center; justify-content:center; \
                         font-size:{:.1}px; font-weight:700; cursor:pointer; \
                         color:{}; background:{}; \
                         border:{:.1}px solid rgba(0,0,0,0.55); \
                         box-shadow:{};",
                        3.0 * scale,
                        10.0 * scale,
                        if i == selected { "#1b1a18" } else { ink.as_str() },
                        if i == selected {
                            "linear-gradient(180deg, #d9d4c8, #b0aa9c)"
                        } else {
                            "linear-gradient(180deg, #3a3a3e, #202024)"
                        },
                        (1.0 * scale).max(1.0),
                        // A pressed button sits *in* the panel.
                        if i == selected {
                            format!("inset 0 {:.1}px {:.1}px rgba(0,0,0,0.45)", 1.5 * scale, 3.0 * scale)
                        } else {
                            format!("0 {:.1}px {:.1}px rgba(0,0,0,0.40)", 1.5 * scale, 3.0 * scale)
                        },
                    ),
                    onclick: {
                        let handle = handle.clone();
                        move |_| {
                            handle.begin_edit();
                            handle.set_normalized(detent(i, count));
                            handle.end_edit();
                        }
                    },
                    "{label}"
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detents_span_the_full_normalized_range() {
        assert_eq!(detent(0, 5), 0.0);
        assert_eq!(detent(4, 5), 1.0);
        assert_eq!(detent(0, 1), 0.0, "a single position rests at the bottom");
    }

    #[test]
    fn a_position_snaps_to_its_nearest_detent() {
        assert_eq!(detent_index(0.0, 5), 0);
        assert_eq!(detent_index(1.0, 5), 4);
        assert_eq!(detent_index(0.26, 5), 1);
        assert_eq!(detent_index(0.74, 5), 3);
    }

    #[test]
    fn detent_round_trips_through_its_index() {
        for count in 2..=6 {
            for i in 0..count {
                assert_eq!(detent_index(detent(i, count), count), i);
            }
        }
    }
}
