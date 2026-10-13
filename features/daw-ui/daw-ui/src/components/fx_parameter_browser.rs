//! FX Parameter Browser — "no-GUI DAW" style parameter list.
//!
//! Shows real DAW FX parameters with live bidirectional updates:
//! 1. Track picker dropdown
//! 2. FX chain list with enable/bypass indicators
//! 3. Scrollable parameter list with sliders
//!
//! Reads from `FX_*` global signals populated by `use_fx_browser_subscription()`.

use crate::prelude::*;
use crate::signals::{
    FX_CHAIN, FX_DAW_CONNECTED, FX_LOADING, FX_PARAMETERS, FX_SELECTED_FX, FX_SELECTED_TRACK,
    FX_TRACKS,
};

// ── Public Component ────────────────────────────────────────────────

/// Top-level FX parameter browser. No props — reads entirely from global signals.
#[component]
pub fn FxParameterBrowser() -> Element {
    let connected = *FX_DAW_CONNECTED.read();

    if !connected {
        return rsx! { FxNotConnected {} };
    }

    rsx! {
        div { class: "h-full w-full flex flex-col bg-card overflow-hidden",
            TrackPicker {}
            FxChainList {}
            FxParameterList {}
        }
    }
}

// ── Not Connected Placeholder ───────────────────────────────────────

#[component]
fn FxNotConnected() -> Element {
    rsx! {
        div { class: "h-full w-full flex flex-col items-center justify-center bg-card p-6 text-center",
            div { class: "text-zinc-500 text-sm mb-2", "No DAW Connected" }
            div { class: "text-zinc-600 text-xs",
                "Connect to REAPER to browse FX parameters."
            }
        }
    }
}

// ── Track Picker ────────────────────────────────────────────────────

#[component]
fn TrackPicker() -> Element {
    let tracks = FX_TRACKS.read();
    let selected = FX_SELECTED_TRACK.read();

    rsx! {
        div { class: "p-3 border-b border-border",
            label { class: "text-[10px] font-semibold text-muted-foreground uppercase tracking-wider mb-1 block",
                "Track"
            }
            select {
                class: "w-full px-2 py-1.5 rounded-lg text-sm bg-muted text-foreground \
                        border border-border focus:outline-none focus:ring-1 focus:ring-accent",
                value: selected.as_deref().unwrap_or(""),
                onchange: move |evt: FormEvent| {
                    let val = evt.value();
                    if val.is_empty() {
                        *FX_SELECTED_TRACK.write() = None;
                    } else {
                        *FX_SELECTED_TRACK.write() = Some(val);
                    }
                },
                option { value: "", "-- Select a track --" }
                for track in tracks.iter() {
                    option {
                        key: "{track.guid}",
                        value: "{track.guid}",
                        selected: selected.as_deref() == Some(&track.guid),
                        "{track.name}"
                    }
                }
            }
        }
    }
}

// ── FX Chain List ───────────────────────────────────────────────────

#[component]
fn FxChainList() -> Element {
    let chain = FX_CHAIN.read();
    let selected_fx = FX_SELECTED_FX.read();
    let selected_track = FX_SELECTED_TRACK.read();

    if selected_track.is_none() {
        return rsx! {};
    }

    if chain.is_empty() {
        return rsx! {
            div { class: "px-3 py-2 text-xs text-muted-foreground italic",
                "No FX on this track"
            }
        };
    }

    rsx! {
        div { class: "border-b border-border",
            div { class: "px-3 pt-3 pb-1",
                h3 { class: "text-[10px] font-semibold text-muted-foreground uppercase tracking-wider",
                    "FX Chain"
                }
            }
            div { class: "px-1 pb-2 flex flex-col gap-0.5 max-h-48 overflow-y-auto",
                for fx in chain.iter() {
                    FxChainItem {
                        key: "{fx.guid}",
                        guid: fx.guid.clone(),
                        name: fx.plugin_name.clone(),
                        plugin_type: format!("{:?}", fx.plugin_type),
                        enabled: fx.enabled,
                        is_selected: selected_fx.as_deref() == Some(&fx.guid),
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct FxChainItemProps {
    guid: String,
    name: String,
    plugin_type: String,
    enabled: bool,
    is_selected: bool,
}

#[component]
fn FxChainItem(props: FxChainItemProps) -> Element {
    let guid = props.guid.clone();
    let selected_class = if props.is_selected {
        "bg-accent/20 ring-1 ring-accent"
    } else {
        "hover:bg-muted"
    };
    let dot_color = if props.enabled {
        "bg-green-400"
    } else {
        "bg-zinc-600"
    };

    rsx! {
        button {
            class: "w-full flex items-center gap-2 px-2 py-1.5 rounded-lg text-left \
                    transition-colors {selected_class}",
            onclick: move |_| {
                *FX_SELECTED_FX.write() = Some(guid.clone());
            },
            // Enable dot
            div { class: "w-2 h-2 rounded-full flex-shrink-0 {dot_color}" }
            // Plugin name
            div { class: "flex-1 min-w-0",
                div { class: "text-xs text-foreground truncate", "{props.name}" }
            }
            // Type badge
            span {
                class: "px-1.5 py-0.5 rounded text-[9px] font-medium uppercase \
                        bg-muted text-muted-foreground flex-shrink-0",
                "{props.plugin_type}"
            }
        }
    }
}

// ── FX Parameter List ───────────────────────────────────────────────

#[component]
fn FxParameterList() -> Element {
    let params = FX_PARAMETERS.read();
    let selected_fx = FX_SELECTED_FX.read();
    let loading = *FX_LOADING.read();

    if selected_fx.is_none() {
        return rsx! {};
    }

    if loading {
        return rsx! {
            div { class: "px-3 py-4 text-xs text-muted-foreground text-center",
                "Loading parameters..."
            }
        };
    }

    // Find the FX name for the header
    let fx_name = {
        let chain = FX_CHAIN.read();
        chain
            .iter()
            .find(|fx| selected_fx.as_deref() == Some(&fx.guid))
            .map(|fx| fx.plugin_name.clone())
            .unwrap_or_default()
    };

    rsx! {
        div { class: "flex-1 flex flex-col overflow-hidden",
            // Header
            div { class: "px-3 pt-3 pb-1 flex items-center justify-between",
                h3 { class: "text-[10px] font-semibold text-muted-foreground uppercase tracking-wider",
                    "Parameters"
                }
                span { class: "text-[10px] text-muted-foreground",
                    "{params.len()} params"
                }
            }
            if !fx_name.is_empty() {
                div { class: "px-3 pb-2",
                    span { class: "text-xs font-medium text-foreground", "{fx_name}" }
                }
            }

            // Scrollable parameter list
            div { class: "flex-1 overflow-y-auto px-3 pb-3",
                div { class: "flex flex-col gap-2",
                    for param in params.iter() {
                        FxParamSlider {
                            key: "{param.index}",
                            index: param.index,
                            name: param.name.clone(),
                            value: param.value,
                            formatted: param.formatted.clone(),
                        }
                    }
                }
            }
        }
    }
}

// ── Parameter Slider ────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
struct FxParamSliderProps {
    index: u32,
    name: String,
    value: f64,
    formatted: String,
}

#[component]
fn FxParamSlider(props: FxParamSliderProps) -> Element {
    let pct = props.value * 100.0;
    let param_index = props.index;

    rsx! {
        div { class: "flex flex-col gap-0.5",
            // Name + formatted value
            div { class: "flex items-center justify-between",
                span { class: "text-xs text-muted-foreground truncate mr-2", "{props.name}" }
                span { class: "text-[10px] font-mono text-foreground flex-shrink-0", "{props.formatted}" }
            }
            // Slider track
            div { class: "relative h-1.5 rounded-full bg-muted overflow-hidden cursor-pointer",
                oninput: move |evt: FormEvent| {
                    if let Ok(val) = evt.value().parse::<f64>() {
                        let clamped = val.clamp(0.0, 1.0);
                        // Update the signal immediately for responsive UI
                        FX_PARAMETERS.write().iter_mut().for_each(|p| {
                            if p.index == param_index {
                                p.value = clamped;
                            }
                        });
                        // Write back to REAPER asynchronously
                        spawn(async move {
                            if let Err(e) = set_daw_parameter(param_index, clamped).await {
                                tracing::warn!("Failed to set parameter {}: {:?}", param_index, e);
                            }
                        });
                    }
                },
                // Fill bar
                div {
                    class: "absolute inset-y-0 left-0 rounded-full bg-accent",
                    style: "width: {pct}%;",
                }
                // Invisible range input for interaction
                input {
                    r#type: "range",
                    class: "absolute inset-0 w-full h-full opacity-0 cursor-pointer",
                    min: "0",
                    max: "1",
                    step: "0.001",
                    value: "{props.value}",
                }
            }
        }
    }
}

/// Write a parameter value back to the DAW.
async fn set_daw_parameter(param_index: u32, value: f64) -> eyre::Result<()> {
    let track_guid = FX_SELECTED_TRACK
        .read()
        .clone()
        .ok_or_else(|| eyre::eyre!("No track selected"))?;
    let fx_guid = FX_SELECTED_FX
        .read()
        .clone()
        .ok_or_else(|| eyre::eyre!("No FX selected"))?;
    let daw = daw_control::Daw::try_get().ok_or_else(|| eyre::eyre!("DAW not connected"))?;

    let project = daw.current_project().await?;
    let track = project
        .tracks()
        .by_guid(&track_guid)
        .await?
        .ok_or_else(|| eyre::eyre!("Track not found"))?;
    let fx = track
        .fx_chain()
        .by_guid(&fx_guid)
        .await?
        .ok_or_else(|| eyre::eyre!("FX not found"))?;
    fx.param(param_index).set(value).await?;
    Ok(())
}
