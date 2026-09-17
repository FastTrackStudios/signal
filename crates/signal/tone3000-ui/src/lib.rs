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
//! rsx! { ToneBrowser { on_loaded: move |i: ToneImport| import(i) } }
//! ```
//!
//! `on_loaded` is the seam to the rig: it fires with a [`ToneImport`] — what
//! was captured, and where the file landed on the engine. The browser
//! deliberately does not reach into the rig itself. It has no business
//! knowing whether it is feeding a preset pool, a drive slot, or a plugin's
//! single amp slot, which is why it reports what the capture *is* and lets
//! the far side decide.

mod art;
mod detail;
mod state;
mod style;

pub use art::ToneArt;
pub use detail::ToneDetail;
pub use state::{Tone3000State, UrlOpener, use_tone3000_state};

use dioxus::prelude::*;
use signal_tone3000_proto::tone3000::Tone3000Client;
use signal_tone3000_proto::{PickedTone, TonePage, ToneQuery, ToneShelf, ToneSummary};

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

/// A downloaded capture, on its way into a rig.
///
/// More than a path, because a rig routes on what was captured: `gear`
/// separates a pedal from an amp, and `group` collects captures of the same
/// piece of gear, so three captures of one pedal can land as three options of
/// one preset rather than three presets holding a third of it each.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToneImport {
    /// The model's display name — the preset or option label.
    pub name: String,
    /// Where the file landed, on the machine running the engine. That is the
    /// right frame of reference even for a GUI on another device: the engine
    /// is what will load it.
    pub path: String,
    /// The catalog's category — `amp`, `amp-cab`, `pedal`, `cab`, …
    pub gear: String,
    /// The Block Preset this capture belongs to — the tone it came from.
    ///
    /// One tone is one capture session of one rig by one creator, and its
    /// models are that session at different settings: a Block Preset and its
    /// variants. Two creators' AC30s are two Presets you choose between, not
    /// variants of each other. See [`grouping_key`].
    pub group: String,
}

/// The Block Preset a downloaded capture belongs to: **the tone it came
/// from**.
///
/// A tone is one capture session — one piece of gear, one signal chain, one
/// creator — and its models are that session at different settings. That is
/// exactly a Block Preset and its variants, so the catalog's own unit of
/// publication is the unit we adopt.
///
/// It is tempting to group by `makes` instead, so that every AC30 lands in one
/// "AC30" preset. That is wrong twice over. It is wrong by the model — a 1964
/// Super Twin through a Neve is not a variant of a 1965 through an SSL, they
/// are different presets you choose between — and it is wrong in practice,
/// because `makes` is not a gear identifier. Creators list the whole chain in
/// it (one AC30 tone names the console, the preamp, two mics and the speaker)
/// and spell the same amp three different ways. Grouping on it would merge
/// captures that should stay apart and split ones that belong together.
///
/// Collecting two tones into one preset is therefore a decision a person
/// makes, not one inferred from a free-text field.
#[must_use]
pub fn grouping_key(tone: &PickedTone) -> String {
    tone.name.trim().to_string()
}

#[cfg(test)]
mod grouping_tests {
    use super::grouping_key;
    use signal_tone3000_proto::PickedTone;

    fn tone(name: &str, makes: &[&str]) -> PickedTone {
        PickedTone {
            name: name.to_string(),
            makes: makes.iter().map(|m| (*m).to_string()).collect(),
            ..PickedTone::default()
        }
    }

    #[test]
    fn two_captures_of_the_same_amp_are_different_presets() {
        // Real catalog rows. Same amp, different year, different room,
        // different creator — things you choose between, not variants of
        // one another.
        let amalgam = tone(
            "1964 VOX AC30 Top Boost Super Twin - Edge of Breakup - A2",
            &["1964 VOX AC30 Top Boost Super Twin"],
        );
        let bennett = tone("1965 VOX AC30 Top Boost", &["1965 Vox AC30"]);
        assert_ne!(grouping_key(&amalgam), grouping_key(&bennett));
    }

    #[test]
    fn makes_is_not_a_gear_identifier() {
        // Clay Bennett's AC30 lists its whole capture chain in `makes` —
        // console, preamp, mics, speaker — so its first entry is not the
        // gear, and grouping on it would be arbitrary.
        let bennett = tone(
            "1965 VOX AC30 Top Boost",
            &[
                "1965 Vox AC30",
                "1987 SSL G Series Console",
                "Celestion Blue",
                "Neve 1073",
                "Royer R-121",
                "Shure SM57",
            ],
        );
        assert_eq!(grouping_key(&bennett), "1965 VOX AC30 Top Boost");
    }

    #[test]
    fn creators_spelling_the_same_amp_differently_do_not_collide() {
        // "VOX AC30 Top boost" and "VOX AC30 Top Boost" are the same string
        // to nobody and the same amp to everybody — which is exactly why the
        // tone, not the spelling, is the identity.
        let roby = tone("RR AC30 TB", &["VOX AC30 Top boost"]);
        let other = tone("Someone Else's AC30", &["VOX AC30 Top Boost"]);
        assert_ne!(grouping_key(&roby), grouping_key(&other));
    }
}

/// The tone browser.
///
/// `on_loaded` fires with a [`ToneImport`] when the user picks a downloaded
/// model for the rig.
#[component]
pub fn ToneBrowser(on_loaded: Callback<ToneImport>) -> Element {
    let client = use_hook(try_consume_context::<Tone3000Client>);
    let opener = use_hook(try_consume_context::<UrlOpener>);
    // Run the hook, THEN provide its value. Passing `use_tone3000_state`
    // straight to `use_context_provider` reads as the same thing and is
    // not: the initializer runs inside the hook machinery, so the hooks
    // inside it try to borrow a hook list that is already borrowed. It
    // panics on first render, in every shell — the plugin host is simply
    // where it was noticed.
    let state = use_tone3000_state();
    use_context_provider(|| state);
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
