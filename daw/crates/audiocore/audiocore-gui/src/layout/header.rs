//! Header bar and status bar for plugin windows.
//!
//! Warm panel with subtle depth and transport indicators.

use crate::controls::segment::SegmentButton;
use crate::theme::{use_theme, Theme, ThemeVariant};
use nice_plug_dioxus::prelude::*;

/// Header bar with plugin title + theme selector + transport info.
#[component]
pub fn Header(
    title: &'static str,
    #[props(default = 120.0)] tempo: f32,
    #[props(default = 4)] time_sig_num: i32,
    #[props(default = 4)] time_sig_den: i32,
    #[props(default = false)] is_playing: bool,
) -> Element {
    let mut theme_signal = use_theme();
    let t = *theme_signal.read();

    let play_bg = if is_playing {
        t.signal_safe
    } else {
        t.toggle_off
    };
    let play_text = if is_playing { "PLAY" } else { "STOP" };
    let play_glow = if is_playing {
        format!("box-shadow:0 0 6px {};", t.signal_safe_glow)
    } else {
        String::new()
    };

    rsx! {
        div {
            style: format!(
                "display:flex; justify-content:space-between; align-items:center; \
                 margin-bottom:{}; padding-bottom:8px; \
                 border-bottom:1px solid {};",
                t.spacing_section,
                t.border,
            ),
            div {
                style: format!(
                    "font-size:{}; font-weight:700; color:{}; \
                     letter-spacing:0.3px;",
                    t.font_size_title,
                    t.text_bright,
                ),
                "{title}"
            }
            div {
                style: "display:flex; gap:2px;",
                for variant in ThemeVariant::ALL {
                    SegmentButton {
                        label: variant.label(),
                        selected: t.variant == variant,
                        on_click: move |_| {
                            theme_signal.set(Theme::for_variant(variant));
                        },
                    }
                }
            }
            div {
                style: format!(
                    "display:flex; gap:12px; align-items:center; \
                     {}",
                    t.style_value(),
                ),
                span {
                    style: format!("color:{};", t.text_dim),
                    "{tempo:.0} BPM"
                }
                span {
                    style: format!("color:{};", t.text_dim),
                    "{time_sig_num}/{time_sig_den}"
                }
                span {
                    style: format!(
                        "padding:2px 8px; border-radius:{}; font-size:11px; \
                         background:{play_bg}; color:#fff; {play_glow}",
                        t.radius_button,
                    ),
                    "{play_text}"
                }
            }
        }
    }
}

/// Status bar showing beat position and optional window size.
#[component]
pub fn StatusBar(beat_position: f32) -> Element {
    let theme_signal = use_theme();
    let t = *theme_signal.read();

    rsx! {
        div {
            style: format!(
                "padding-top:6px; margin-top:8px; border-top:1px solid {}; \
                 {} color:{};",
                t.border,
                t.style_value(),
                t.text_dim,
            ),
            {
                let info = if let Some(state) = try_use_context::<std::sync::Arc<DioxusState>>() {
                    let (w, h) = state.size();
                    format!("Beat: {beat_position:.2} | {w}x{h}")
                } else {
                    format!("Beat: {beat_position:.2}")
                };
                rsx! { "{info}" }
            }
        }
    }
}
