//! FX panel — embedded EQ and Compressor GUI.

use nice_plug_dioxus::prelude::*;

use crate::ProcessingChain;
use audiocore_gui::meters::GrMeter;
use audiocore_gui::viz::{EqBand, EqBandShape, EqGraph};

// ── Top-level FX view ──────────────────────────────────────────────────────────

/// Top-level FX view — renders EQ and Compressor panels side-by-side.
///
/// Gets the [`ProcessingChain`] from Dioxus context (injected by the desktop
/// app's `App` component). Renders a placeholder if no chain is available.
#[component]
pub fn FxView() -> Element {
    let chain = use_context::<ProcessingChain>();

    rsx! {
        div {
            style: "display:flex; flex-direction:row; gap:16px; width:100%; height:100%; \
                    padding:16px; box-sizing:border-box; overflow:auto;",
            EqPanel { chain: chain.clone() }
            CompPanel { chain: chain.clone() }
        }
    }
}

// ── EQ panel ──────────────────────────────────────────────────────────────────

#[component]
fn EqPanel(chain: ProcessingChain) -> Element {
    let _ = chain;
    let mut bands: Signal<Vec<EqBand>> = use_signal(|| {
        vec![
            EqBand {
                index: 0,
                used: true,
                enabled: true,
                frequency: 80.0,
                gain: 0.0,
                q: 0.707,
                shape: EqBandShape::LowCut,
                ..Default::default()
            },
            EqBand {
                index: 1,
                used: true,
                enabled: true,
                frequency: 500.0,
                gain: 0.0,
                q: 1.0,
                shape: EqBandShape::Bell,
                ..Default::default()
            },
            EqBand {
                index: 2,
                used: true,
                enabled: true,
                frequency: 8000.0,
                gain: 0.0,
                q: 0.707,
                shape: EqBandShape::HighShelf,
                ..Default::default()
            },
        ]
    });

    let mut on_band_change = move |(idx, band): (usize, EqBand)| {
        let mut bs = bands.write();
        if idx < bs.len() {
            bs[idx] = band.clone();
        }
    };

    let mut on_band_add = move |band: EqBand| {
        let mut new_band = band.clone();
        new_band.index = bands.read().len();
        bands.write().push(new_band);
    };

    let mut on_band_remove = move |idx: usize| {
        let mut bs = bands.write();
        if idx < bs.len() {
            bs.remove(idx);
            // Re-index
            for (i, b) in bs.iter_mut().enumerate() {
                b.index = i;
            }
        }
    };

    rsx! {
        div {
            style: "flex:1; min-width:0; background:#1a1a1a; border-radius:8px; \
                    padding:16px; box-sizing:border-box; display:flex; flex-direction:column; gap:8px;",
            div {
                style: "font-size:12px; font-weight:600; color:#a1a1aa; text-transform:uppercase; \
                        letter-spacing:0.08em;",
                "EQ"
            }
            EqGraph {
                bands: bands,
                sample_rate: 48000.0,
                on_band_change: move |(idx, band)| on_band_change((idx, band)),
                on_band_add: move |band| on_band_add(band),
                on_band_remove: move |idx| on_band_remove(idx),
            }
        }
    }
}

// ── Compressor panel ───────────────────────────────────────────────────────────

#[component]
fn CompPanel(chain: ProcessingChain) -> Element {
    let mut threshold = use_signal(|| -18.0f64);
    let mut ratio = use_signal(|| 4.0f64);
    let mut attack = use_signal(|| 10.0f64);
    let mut release = use_signal(|| 100.0f64);
    let mut auto_makeup = use_signal(|| false);

    let gr = chain.gain_reduction_db();

    rsx! {
        div {
            style: "width:260px; flex-shrink:0; background:#1a1a1a; border-radius:8px; \
                    padding:16px; box-sizing:border-box; display:flex; flex-direction:column; gap:12px;",

            // Title
            div {
                style: "font-size:12px; font-weight:600; color:#a1a1aa; text-transform:uppercase; \
                        letter-spacing:0.08em;",
                "Compressor"
            }

            // GR Meter + params row
            div {
                style: "display:flex; flex-direction:row; gap:12px; align-items:flex-start;",

                // GR meter
                GrMeter {
                    gain_reduction_db: gr,
                    height: 150.0,
                }

                // Parameter sliders
                div {
                    style: "flex:1; display:flex; flex-direction:column; gap:10px;",

                    // Threshold
                    ParamRow {
                        label: "Threshold",
                        value: *threshold.read(),
                        min: -60.0,
                        max: 0.0,
                        unit: " dB",
                        on_change: move |v: f64| {
                            threshold.set(v);
                        },
                    }

                    // Ratio
                    ParamRow {
                        label: "Ratio",
                        value: *ratio.read(),
                        min: 1.0,
                        max: 20.0,
                        unit: ":1",
                        on_change: move |v: f64| {
                            ratio.set(v);
                        },
                    }

                    // Attack
                    ParamRow {
                        label: "Attack",
                        value: *attack.read(),
                        min: 0.1,
                        max: 200.0,
                        unit: " ms",
                        on_change: move |v: f64| {
                            attack.set(v);
                        },
                    }

                    // Release
                    ParamRow {
                        label: "Release",
                        value: *release.read(),
                        min: 5.0,
                        max: 2000.0,
                        unit: " ms",
                        on_change: move |v: f64| {
                            release.set(v);
                        },
                    }
                }
            }

            // Auto Makeup toggle
            div {
                style: "display:flex; align-items:center; gap:8px;",
                button {
                    style: {
                        let active = *auto_makeup.read();
                        if active {
                            "padding:4px 10px; border-radius:4px; font-size:11px; font-weight:600; \
                             background:#3b82f6; color:#fff; border:none; cursor:pointer;"
                        } else {
                            "padding:4px 10px; border-radius:4px; font-size:11px; font-weight:600; \
                             background:#27272a; color:#a1a1aa; border:1px solid #3f3f46; cursor:pointer;"
                        }
                    },
                    onclick: move |_| {
                        let new_val = !*auto_makeup.read();
                        auto_makeup.set(new_val);
                    },
                    "Auto Makeup"
                }
            }
        }
    }
}

// ── Helper: labeled parameter row with Slider ─────────────────────────────────

#[component]
fn ParamRow(
    label: &'static str,
    value: f64,
    min: f64,
    max: f64,
    unit: &'static str,
    on_change: EventHandler<f64>,
) -> Element {
    let display = format!("{:.1}{unit}", value);

    rsx! {
        div {
            style: "display:flex; flex-direction:column; gap:3px;",
            div {
                style: "display:flex; justify-content:space-between; align-items:baseline;",
                span {
                    style: "font-size:10px; color:#71717a; text-transform:uppercase; letter-spacing:0.06em;",
                    "{label}"
                }
                span {
                    style: "font-size:11px; color:#e4e4e7; font-variant-numeric:tabular-nums;",
                    "{display}"
                }
            }
            audiocore_gui::controls::Slider {
                value: value,
                min: min,
                max: max,
                on_change: Some(Callback::new(move |v: f64| on_change.call(v))),
            }
        }
    }
}
