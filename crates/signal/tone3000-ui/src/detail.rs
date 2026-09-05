//! One tone, in full: the photographs, the write-up, and its models.
//!
//! This is where a download starts, so it is also where attribution has to be
//! unmissable — creator and licence sit next to the button, not in a footer.
//! The API terms require the credit to survive; showing it at the moment of
//! the download is also just the honest place for it.

use dioxus::prelude::*;
use signal_tone3000_proto::tone3000::Tone3000Client;
use signal_tone3000_proto::{PickedTone, ToneModel};

use crate::art::ToneArt;
use crate::state::{Tone3000State, UrlOpener};
use crate::style;

/// The detail panel. `on_loaded` fires when a model has finished downloading
/// and the user asked for it in the rig, with `(display name, engine path)`.
#[component]
pub fn ToneDetail(
    tone: PickedTone,
    on_close: Callback<()>,
    on_loaded: Callback<(String, String)>,
) -> Element {
    let opener = use_hook(try_consume_context::<UrlOpener>);

    rsx! {
        div {
            style: "position:absolute;inset:0;background:rgba(8,8,10,0.72);
                    display:flex;justify-content:center;align-items:flex-start;
                    padding:24px;overflow-y:auto;z-index:20;",
            onclick: move |_| on_close.call(()),

            div {
                style: "width:100%;max-width:680px;background:{style::PANEL};
                        border:1px solid {style::BORDER};border-radius:10px;
                        overflow:hidden;color:{style::TEXT};",
                // The backdrop closes; the panel must not close when the
                // user clicks inside it to select text or press a button.
                onclick: move |e| e.stop_propagation(),

                ToneArt {
                    url: tone.images.first().cloned().unwrap_or_default(),
                    height: "220px".to_string(),
                    label: tone.name.clone(),
                }

                div { style: "padding:18px 20px 22px;",
                    div { style: "display:flex;align-items:flex-start;gap:12px;",
                        div { style: "flex:1;min-width:0;",
                            div { style: "font-size:18px;font-weight:600;line-height:1.2;",
                                "{tone.name}"
                            }
                            div { style: "margin-top:4px;font-size:13px;color:{style::MUTED};",
                                if tone.creator.is_empty() {
                                    "creator not stated"
                                } else {
                                    "by {tone.creator}"
                                }
                            }
                        }
                        button {
                            style: style::ghost_button(),
                            onclick: move |_| on_close.call(()),
                            "Close"
                        }
                    }

                    div { style: "display:flex;flex-wrap:wrap;gap:6px;margin-top:12px;",
                        if !tone.gear.is_empty() {
                            span { style: style::chip(style::ACCENT), "{tone.gear}" }
                        }
                        for make in tone.makes.iter() {
                            span { style: style::chip(style::MUTED), "{make}" }
                        }
                        for tag in tone.tags.iter() {
                            span { style: style::chip(style::MUTED), "{tag}" }
                        }
                        if !tone.license.is_empty() {
                            span { style: style::chip(style::MUTED), "licence: {tone.license}" }
                        }
                    }

                    if !tone.description.is_empty() {
                        p {
                            style: "margin:14px 0 0;font-size:13px;line-height:1.5;
                                    color:{style::MUTED};white-space:pre-wrap;",
                            "{tone.description}"
                        }
                    }

                    // The tone's own page. Required by the terms, and useful:
                    // demos, comments and the rest live there, not here.
                    if !tone.tone_url.is_empty() {
                        {
                            let url = tone.tone_url.clone();
                            let opener = opener.clone();
                            rsx! {
                                button {
                                    style: "{style::ghost_button()}margin-top:12px;",
                                    onclick: move |_| {
                                        if let Some(o) = &opener { o.open(url.clone()); }
                                    },
                                    "View on TONE3000 ↗"
                                }
                            }
                        }
                    }

                    div {
                        style: "margin-top:18px;font-size:11px;letter-spacing:0.08em;
                                text-transform:uppercase;color:{style::MUTED};",
                        if tone.models.is_empty() {
                            "no models"
                        } else {
                            "{tone.models.len()} models"
                        }
                    }

                    div { style: "margin-top:8px;display:flex;flex-direction:column;gap:6px;",
                        for model in tone.models.iter() {
                            ModelRow {
                                key: "{model.id}",
                                tone_id: tone.id.clone(),
                                model: model.clone(),
                                on_loaded,
                            }
                        }
                    }
                }
            }
        }
    }
}

/// One downloadable variant: what it is, and the one button that matters.
#[component]
fn ModelRow(
    tone_id: String,
    model: ToneModel,
    on_loaded: Callback<(String, String)>,
) -> Element {
    let client = use_hook(try_consume_context::<Tone3000Client>);
    let state = use_context::<Tone3000State>();
    let done = state.completed(&model.id);
    let in_flight = state.in_flight(&model.id);
    let percent = state.percent(&model.id);
    let failed = state
        .downloads
        .read()
        .get(&model.id)
        .filter(|p| p.done && !p.error.is_empty())
        .map(|p| p.error.clone());

    let name = if model.name.is_empty() {
        format!("model {}", model.id)
    } else {
        model.name.clone()
    };

    rsx! {
        div {
            style: "display:flex;align-items:center;gap:10px;padding:8px 10px;
                    background:{style::ROW};border:1px solid {style::BORDER};
                    border-radius:6px;",
            div { style: "flex:1;min-width:0;",
                div {
                    style: "font-size:13px;overflow:hidden;text-overflow:ellipsis;
                            white-space:nowrap;",
                    "{name}"
                }
                div { style: "font-size:11px;color:{style::MUTED};margin-top:2px;",
                    if model.size.is_empty() { "" } else { "{model.size}" }
                    if !model.size.is_empty() && !model.architecture.is_empty() { " · " }
                    if model.architecture.is_empty() { "" } else { "architecture {model.architecture}" }
                }
                if let Some(error) = failed {
                    div { style: "font-size:11px;color:{style::DANGER};margin-top:2px;", "{error}" }
                }
            }

            if let Some(finished) = done {
                {
                    let label = name.clone();
                    let path = finished.path.clone();
                    rsx! {
                        button {
                            style: style::primary_button(),
                            onclick: move |_| on_loaded.call((label.clone(), path.clone())),
                            "Use in rig"
                        }
                    }
                }
            } else if in_flight {
                div { style: "min-width:92px;text-align:right;font-size:12px;color:{style::MUTED};",
                    // No length from the server is common; saying "0%" for a
                    // download that is moving is a lie a progress bar tells.
                    match percent {
                        Some(p) => rsx! { "{p}%" },
                        None => rsx! { "downloading…" },
                    }
                }
            } else {
                {
                    let (client, tone_id, model_id) =
                        (client.clone(), tone_id.clone(), model.id.clone());
                    rsx! {
                        button {
                            style: style::primary_button(),
                            onclick: move |_| {
                                let (client, tone_id, model_id) =
                                    (client.clone(), tone_id.clone(), model_id.clone());
                                spawn(async move {
                                    if let Some(client) = client {
                                        let _ = client.download_model(tone_id, model_id).await;
                                    }
                                });
                            },
                            "Download"
                        }
                    }
                }
            }
        }
    }
}
