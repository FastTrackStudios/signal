//! The TONE3000 tone browser — search, look at, and download NAM captures
//! without leaving the rig.
//!
//! One component tree for every surface. It renders purely from the wire
//! contract via the generated vox clients, so it mounts identically in the
//! browser remote, the desktop app and a plugin editor; the engine holds the
//! session and does the fetching. See `signal-tone3000-proto` for why the
//! sign-in is a three-step conversation and why artwork travels as bytes.
//!
//! # Mounting it
//!
//! Provide a [`Tone3000Client`] (and, for live download progress, a
//! `Tone3000StreamClient`) plus a [`UrlOpener`] in context, then render
//! [`ToneBrowser`]:
//!
//! ```ignore
//! use_context_provider(|| client.clone());
//! use_context_provider(|| UrlOpener::new(|url| open_in_system_browser(&url)));
//! rsx! { ToneBrowser { on_loaded: move |(name, path)| add_preset(name, path) } }
//! ```
//!
//! `on_loaded` is the seam to the rig: it fires with a display name and the
//! path the model landed at on the engine, which is exactly what
//! `Rig::add_preset` / `Rig::set_preset_nam` take. The browser deliberately
//! does not reach into the rig itself — it has no business knowing whether it
//! is feeding a preset, a block, or a plugin's single amp slot.

mod art;
mod detail;
mod state;
mod style;

pub use art::ToneArt;
pub use detail::ToneDetail;
pub use state::{Tone3000State, UrlOpener, use_tone3000_state};

use dioxus::prelude::*;
use signal_tone3000_proto::tone3000::Tone3000Client;
use signal_tone3000_proto::{TonePage, ToneQuery, ToneShelf, ToneSummary};

/// What the grid is currently showing.
#[derive(Clone, PartialEq)]
enum View {
    /// One of the catalog's bounded lists — what the browser opens on,
    /// because it costs no search quota and a screen of real tones is a far
    /// better invitation than an empty search box.
    Shelf(ToneShelf),
    /// A search the user typed.
    Search(String),
}

/// Gear filters offered as tabs. Not the API's whole vocabulary — these are
/// the categories a guitar rig actually loads.
const GEARS: [(&str, &str); 4] = [
    ("", "All"),
    ("amp", "Amps"),
    ("amp-cab", "Amp + cab"),
    ("pedal", "Pedals"),
];

/// The tone browser.
///
/// `on_loaded` fires with `(display name, engine path)` when the user picks a
/// downloaded model for the rig.
#[component]
pub fn ToneBrowser(on_loaded: Callback<(String, String)>) -> Element {
    let client = use_hook(try_consume_context::<Tone3000Client>);
    let opener = use_hook(try_consume_context::<UrlOpener>);
    let state = use_context_provider(use_tone3000_state);
    art::use_art_cache();

    let mut view = use_signal(|| View::Shelf(ToneShelf::Trending));
    let mut gear = use_signal(String::new);
    let mut query_text = use_signal(String::new);
    // The open tone is held as an id, and the detail is a resource over it.
    // Fetching from inside the grid's loop would move the client once per
    // card; an id is `Copy`-cheap and the fetch happens in one place.
    let mut open_id = use_signal(|| None::<String>);
    let mut sign_in_url = use_signal(String::new);

    let signed_in = state.status.read().signed_in;

    // The listing. Re-runs when the view, the gear filter, or the session
    // changes — signing in is the difference between an error and a catalog.
    let list_client = client.clone();
    let page = use_resource(use_reactive!(|(view, gear, signed_in)| {
        let client = list_client.clone();
        let (view, gear) = (view.read().clone(), gear.read().clone());
        async move {
            let Some(client) = client else {
                return TonePage {
                    error: "no engine".into(),
                    ..TonePage::default()
                };
            };
            if !signed_in {
                return TonePage::default();
            }
            let result = match view {
                View::Shelf(shelf) => client.shelf(shelf, 1).await,
                View::Search(text) => {
                    client
                        .search(ToneQuery {
                            text,
                            gears: if gear.is_empty() { vec![] } else { vec![gear] },
                            format: "nam".into(),
                            ..ToneQuery::default()
                        })
                        .await
                }
            };
            result.unwrap_or_else(|e| TonePage {
                error: e.to_string(),
                ..TonePage::default()
            })
        }
    }));

    let detail_client = client.clone();
    let detail = use_resource(move || {
        let client = detail_client.clone();
        let id = open_id();
        async move {
            let id = id?;
            client?.tone(id).await.ok()
        }
    });

    rsx! {
        div {
            style: "position:relative;display:flex;flex-direction:column;height:100%;
                    min-height:0;background:{style::PANEL};color:{style::TEXT};
                    font:13px/1.4 system-ui,-apple-system,sans-serif;",

            // ── Header: session, search, filters ──────────────────────
            div {
                style: "display:flex;flex-direction:column;gap:10px;padding:12px 14px;
                        border-bottom:1px solid {style::BORDER};",

                div { style: "display:flex;align-items:center;gap:10px;",
                    div { style: "font-weight:600;", "TONE3000" }
                    div { style: "flex:1;" }
                    if signed_in {
                        {
                            let name = state.status.read().username.clone();
                            let client = client.clone();
                            rsx! {
                                span { style: "font-size:12px;color:{style::MUTED};",
                                    if name.is_empty() { "signed in" } else { "{name}" }
                                }
                                button {
                                    style: style::ghost_button(),
                                    onclick: move |_| {
                                        let client = client.clone();
                                        let status = state.status;
                                        spawn(async move {
                                            if let Some(c) = &client {
                                                let _ = c.sign_out().await;
                                            }
                                            state::refresh_status(client, status);
                                        });
                                    },
                                    "Sign out"
                                }
                            }
                        }
                    } else {
                        {
                            let client = client.clone();
                            let opener = opener.clone();
                            rsx! {
                                button {
                                    style: style::primary_button(),
                                    onclick: move |_| {
                                        let (client, opener) = (client.clone(), opener.clone());
                                        spawn(async move {
                                            let Some(c) = client else { return };
                                            // `false`: our own search is the
                                            // browser below. The picker flow
                                            // is a separate button.
                                            let Ok(request) = c.begin_sign_in(false).await else {
                                                return;
                                            };
                                            if request.authorize_url.is_empty() {
                                                sign_in_url.set(
                                                    "This build has no TONE3000 key configured."
                                                        .into(),
                                                );
                                                return;
                                            }
                                            match opener {
                                                Some(o) => o.open(request.authorize_url),
                                                // No opener: show the URL so
                                                // the user can carry it to a
                                                // browser themselves.
                                                None => sign_in_url.set(request.authorize_url),
                                            }
                                        });
                                    },
                                    "Sign in"
                                }
                            }
                        }
                    }
                }

                if !sign_in_url.read().is_empty() {
                    div {
                        style: "font-size:11px;color:{style::MUTED};word-break:break-all;
                                background:{style::ROW};border:1px solid {style::BORDER};
                                border-radius:5px;padding:8px;",
                        "{sign_in_url}"
                    }
                }

                div { style: "display:flex;align-items:center;gap:8px;",
                    input {
                        style: style::input(),
                        r#type: "text",
                        placeholder: "Search the catalog — amp, pedal, creator…",
                        value: "{query_text}",
                        oninput: move |e| query_text.set(e.value()),
                        onkeydown: move |e| {
                            if e.key() == Key::Enter {
                                let text = query_text.read().clone();
                                view.set(if text.trim().is_empty() {
                                    View::Shelf(ToneShelf::Trending)
                                } else {
                                    View::Search(text)
                                });
                            }
                        },
                    }
                    button {
                        style: style::primary_button(),
                        onclick: move |_| {
                            let text = query_text.read().clone();
                            view.set(if text.trim().is_empty() {
                                View::Shelf(ToneShelf::Trending)
                            } else {
                                View::Search(text)
                            });
                        },
                        "Search"
                    }
                }

                div { style: "display:flex;flex-wrap:wrap;gap:6px;",
                    for (value, label) in GEARS {
                        {
                            let selected_gear = gear.read().clone() == value;
                            rsx! {
                                button {
                                    key: "{label}",
                                    style: style::tab(selected_gear),
                                    onclick: move |_| gear.set(value.to_string()),
                                    "{label}"
                                }
                            }
                        }
                    }
                    div { style: "flex:1;" }
                    for (shelf, label) in [
                        (ToneShelf::Trending, "Trending"),
                        (ToneShelf::Latest, "Latest"),
                        (ToneShelf::Favorited, "Favourites"),
                    ] {
                        {
                            let is_current = *view.read() == View::Shelf(shelf);
                            rsx! {
                                button {
                                    key: "{label}",
                                    style: style::tab(is_current),
                                    onclick: move |_| {
                                        query_text.set(String::new());
                                        view.set(View::Shelf(shelf));
                                    },
                                    "{label}"
                                }
                            }
                        }
                    }
                }
            }

            // ── The grid ──────────────────────────────────────────────
            div { style: "flex:1;min-height:0;overflow-y:auto;padding:12px 14px;",
                if !signed_in {
                    div { style: "padding:32px 8px;text-align:center;color:{style::MUTED};",
                        "Sign in to TONE3000 to browse and download captures."
                    }
                } else {
                    match page.read().as_ref() {
                        None => rsx! {
                            div { style: "padding:32px 8px;text-align:center;color:{style::MUTED};",
                                "Loading…"
                            }
                        },
                        // An error and an empty result are different things,
                        // and a rate limit must never look like "no tones
                        // matched".
                        Some(p) if !p.error.is_empty() => rsx! {
                            div { style: "padding:32px 8px;text-align:center;color:{style::DANGER};",
                                "{p.error}"
                            }
                        },
                        Some(p) if p.tones.is_empty() => rsx! {
                            div { style: "padding:32px 8px;text-align:center;color:{style::MUTED};",
                                "Nothing here."
                            }
                        },
                        Some(p) => rsx! {
                            div {
                                style: "display:grid;gap:12px;
                                        grid-template-columns:repeat(auto-fill,minmax(200px,1fr));",
                                for tone in p.tones.iter() {
                                    ToneCard {
                                        key: "{tone.id}",
                                        tone: tone.clone(),
                                        on_open: move |id: String| open_id.set(Some(id)),
                                    }
                                }
                            }
                        },
                    }
                }
            }

            // The credit the API terms ask for, and the way back to the site.
            div {
                style: "padding:8px 14px;border-top:1px solid {style::BORDER};
                        font-size:11px;color:{style::MUTED};display:flex;
                        align-items:center;gap:8px;",
                "Powered by TONE3000"
                div { style: "flex:1;" }
                {
                    let opener = opener.clone();
                    rsx! {
                        button {
                            style: style::ghost_button(),
                            onclick: move |_| {
                                if let Some(o) = &opener {
                                    o.open("https://www.tone3000.com");
                                }
                            },
                            "tone3000.com ↗"
                        }
                    }
                }
            }

            if let Some(Some(tone)) = detail.read().clone() {
                ToneDetail {
                    tone,
                    on_close: move |()| open_id.set(None),
                    on_loaded,
                }
            }
        }
    }
}

/// One tone in the grid.
#[component]
fn ToneCard(tone: ToneSummary, on_open: Callback<String>) -> Element {
    let id = tone.id.clone();
    rsx! {
        button {
            style: "display:flex;flex-direction:column;text-align:left;padding:0;
                    background:{style::ROW};border:1px solid {style::BORDER};
                    border-radius:7px;overflow:hidden;cursor:pointer;color:inherit;",
            onclick: move |_| on_open.call(id.clone()),

            ToneArt {
                url: tone.image.clone(),
                height: "112px".to_string(),
                label: tone.title.clone(),
            }

            div { style: "padding:9px 10px 10px;display:flex;flex-direction:column;gap:3px;",
                div {
                    style: "font-size:13px;font-weight:600;line-height:1.25;
                            display:-webkit-box;-webkit-line-clamp:2;
                            -webkit-box-orient:vertical;overflow:hidden;",
                    "{tone.title}"
                }
                div {
                    style: "font-size:11px;color:{style::MUTED};overflow:hidden;
                            text-overflow:ellipsis;white-space:nowrap;",
                    if tone.creator.is_empty() { "" } else { "{tone.creator}" }
                }
                div { style: "font-size:11px;color:{style::MUTED};margin-top:2px;",
                    "{tone.models_count} models · {tone.downloads_count} downloads"
                }
            }
        }
    }
}
