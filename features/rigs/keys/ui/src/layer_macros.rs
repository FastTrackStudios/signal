//! **Layer macros** — the layer's Global Controls, Omnisphere's Main page.
//!
//! One Filter, one Envelope pair, Vibrato, Unison and Ambience that reach
//! *every audible module beneath the layer*, plus the layer's own Tone EQ,
//! Limiter and FX bypass (those sit at the layer output, after the modules
//! have summed).
//!
//! A control is **absolute** while the audible modules agree on the
//! parameter — the knob is 1:1 with it. The moment they differ it becomes a
//! **bipolar offset**: centre is the modules' own settings, and turning it
//! scales all of them toward the parameter's floor or ceiling together. Come
//! back to centre and the patch is exactly as it was. The spread under each
//! section says what the modules currently hold ("0.4 – 2.1 kHz"), so a
//! centred knob is never a mystery.

use dioxus::prelude::*;
use signal_keys_proto::{KeysLayerDetail, KeysMacro};

use crate::fader::{fmt_db, Fader};
use crate::graphs::{module_color, Adsr, ModuleCurve};
use crate::module_edit::{KnobRow, Panel};

/// Every module as an overlay curve — its envelopes, its filter, its colour.
fn curves(detail: &KeysLayerDetail) -> Vec<ModuleCurve> {
    detail
        .modules
        .iter()
        .map(|m| ModuleCurve {
            slot: m.slot.clone(),
            color: module_color(m.index).to_string(),
            amp: Adsr {
                attack_ms: m.amp_env.attack_ms,
                decay_ms: m.amp_env.decay_ms,
                sustain: m.amp_env.sustain,
                release_ms: m.amp_env.release_ms,
            },
            filter: Adsr {
                attack_ms: m.filter_env.attack_ms,
                decay_ms: m.filter_env.decay_ms,
                sustain: m.filter_env.sustain,
                release_ms: m.filter_env.release_ms,
            },
            cutoff_hz: m.cutoff_hz,
            resonance: m.resonance,
            unison: m.unison,
            detune: m.detune,
            vib_rate: m.vib_rate,
            vib_depth: m.vib_depth,
            vib_delay_ms: m.vib_delay_ms,
            dim: !m.enabled || !m.live,
            // Everything here belongs to the lane being edited — the zoom has
            // nothing out of scope to push into the background.
            focus: true,
        })
        .collect()
}

/// The panels below the Filter and Amp cards — those two own their own
/// graphs and knob rows.
const GROUPS: &[(&str, &str)] = &[
    ("Ambience", "reverb amount · length"),
    ("Tone", "the layer's EQ — centred is bypassed"),
    ("Effects", "layer output"),
];

/// The layer's Global Controls + the module strip they act on.
#[component]
pub fn LayerMacros(
    detail: KeysLayerDetail,
    accent: String,
    on_global: EventHandler<(String, f32)>,
    on_module_gain: EventHandler<(u32, f32)>,
    on_module_enabled: EventHandler<(u32, bool)>,
    /// Zoom into a module (the surface these macros are driving).
    on_open_module: EventHandler<u32>,
) -> Element {
    let group = |name: &str| -> Vec<KeysMacro> {
        detail
            .layer_macros
            .iter()
            .filter(|m| m.group == name)
            .cloned()
            .collect()
    };
    // One spread line per panel: the first spanning macro speaks for it.
    let spread = |name: &str| -> Option<String> {
        detail
            .layer_macros
            .iter()
            .find(|m| m.group == name && !m.spread.is_empty())
            .map(|m| m.spread.clone())
    };
    let varies = |name: &str| {
        detail
            .layer_macros
            .iter()
            .any(|m| m.group == name && m.bipolar)
    };

    // One spacing scale for the page: 4 inside a label block, 8/12 inside a
    // panel, 16 between panels, 24 between the three bands (modules ·
    // Filter+Amp · Global Controls). Everything at 12 was why nothing read as
    // grouped.
    rsx! {
        div {
            style: "flex: 1; min-height: 0; overflow: auto; padding: 16px 18px 24px; display: flex; \
                    flex-direction: column; gap: 24px;",

            // ── The modules these macros drive ──────────────────────────
            div {
                style: "display: flex; gap: 12px; align-items: stretch; flex-wrap: wrap;",
                for m in detail.modules.iter() {
                    {
                        let idx = m.index;
                        let on = m.enabled;
                        let name = if m.patch.is_empty() { "— empty —".to_string() } else { m.patch.clone() };
                        rsx! {
                            div {
                                key: "{m.index}",
                                style: format!(
                                    "display: flex; gap: 14px; padding: 12px 14px; min-width: 230px; \
                                     border: 1px solid {}; border-radius: 12px; background: #0d0d10;",
                                    if m.live && on { "#1f2b3a" } else { "#1c1c21" },
                                ),
                                div { style: "display: flex; flex-direction: column; gap: 6px; min-width: 0; flex: 1;",
                                    div { style: "display: flex; align-items: center; gap: 8px;",
                                        span {
                                            style: format!(
                                                "font-size: 10px; font-weight: 700; color: {};",
                                                if m.live && on { accent.clone() } else { "#3f3f46".to_string() },
                                            ),
                                            "{m.slot}"
                                        }
                                        button {
                                            style: format!(
                                                "appearance: none; border: 1px solid {}; border-radius: 5px; \
                                                 padding: 1px 7px; font-size: 8px; font-weight: 700; \
                                                 background: {}; color: {};",
                                                if on { "#166534" } else { "#3f3f46" },
                                                if on { "#0f2417" } else { "#131316" },
                                                if on { "#4ade80" } else { "#52525b" },
                                            ),
                                            onclick: move |_| on_module_enabled.call((idx, !on)),
                                            if on { "ON" } else { "OFF" }
                                        }
                                        div { style: "flex: 1;" }
                                        button {
                                            style: "appearance: none; border: none; background: transparent; \
                                                    color: #52525b; font-size: 9px; font-weight: 700;",
                                            onclick: move |_| on_open_module.call(idx),
                                            "EDIT →"
                                        }
                                    }
                                    span {
                                        style: format!(
                                            "font-size: 11px; font-weight: 600; line-height: 1.2; color: {}; \
                                             overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                                            if m.live { "#e4e4e7" } else { "#52525b" },
                                        ),
                                        "{name}"
                                    }
                                    span { style: "font-size: 9px; color: #52525b;", {fmt_db(m.gain_db)} }
                                }
                                Fader {
                                    db: m.gain_db,
                                    height_px: 74,
                                    accent: accent.clone(),
                                    dimmed: !on || !m.live,
                                    on_change: move |db: f32| on_module_gain.call((idx, db)),
                                }
                            }
                        }
                    }
                }
            }

            // ── The shapes: the same three cards the mixer's band shows,
            // with room to breathe.
            div {
                style: "display: grid; gap: 16px; \
                        grid-template-columns: repeat(auto-fit, minmax(360px, 1fr));",
                for (i, shape) in [
                    crate::macro_panel::Shape::Unison,
                    crate::macro_panel::Shape::Vibrato,
                    crate::macro_panel::Shape::FilterResponse,
                    crate::macro_panel::Shape::FilterEnv,
                    crate::macro_panel::Shape::AmpEnv,
                ]
                .into_iter()
                .enumerate()
                {
                    crate::macro_panel::ShapeCard {
                        key: "{i}",
                        shape,
                        macros: detail.layer_macros.clone(),
                        curves: curves(&detail),
                        accent: accent.clone(),
                        height_px: 180,
                        on_change: move |(id, v)| on_global.call((id, v)),
                    }
                }
            }

            // ── The Global Controls ─────────────────────────────────────
            //
            // `auto-fill`, not `auto-fit`: a panel holding three knobs should
            // stay knob-width and leave the rest of the row empty, instead of
            // stretching to the window with its controls stranded at the left.
            div {
                style: "display: grid; gap: 16px; align-content: start; \
                        grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));",
                for (name, hint) in GROUPS.iter() {
                    {
                        let macros = group(name);
                        if macros.is_empty() {
                            rsx! {}
                        } else {
                            let spread = spread(name);
                            let bipolar = varies(name);
                            rsx! {
                                Panel {
                                    key: "{name}",
                                    title: name.to_string(),
                                    accent: accent.clone(),
                                    lit: macros.iter().any(|m| m.live),
                                    trailing: rsx! {
                                        if bipolar {
                                            span { style: "font-size: 9px; color: #fbbf24;", "offset" }
                                        }
                                    },
                                    div { style: "display: flex; flex-direction: column; gap: 12px;",
                                        KnobRow {
                                            macros: macros.clone(),
                                            accent: accent.clone(),
                                            on_change: move |(id, v)| on_global.call((id, v)),
                                        }
                                        span { style: "font-size: 9px; color: #52525b; line-height: 1.4;",
                                            if let Some(s) = spread.clone() { "modules: {s}" } else { "{hint}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
