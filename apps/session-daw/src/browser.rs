//! The browser: what to load on the selected track, down the window's
//! left (`b`).
//!
//! It follows the selection. On a guitar or a bass: signal's rig — with
//! none there yet, the profiles its instrument's library has, to load one
//! (`RIG_LOAD`); with one, its profiles to switch to and each of its
//! stacks' patches, the one playing lit. A click on a patch plays it now
//! (`TONE_SET`); a Shift-click puts a switch to it into the song at the
//! cursor (`TONE_INSERT`), so it switches there every time the song plays.
//! Every press is the action a model runs.

use dioxus::prelude::*;

use crate::shell::{BAR_BG, DIM, RULE};

/// How often the browser reads the selection and its rig again, ms.
const READ_EVERY_MS: u64 = 400;

/// How often it looks for a toggle asked for on the Action List, ms.
const TOGGLE_EVERY_MS: u64 = 100;

/// Show or hide the browser: what to load on the selected track.
pub static BROWSER: architect::action::ActionMeta = crate::actions::meta(
    "SESSION_BROWSER",
    "browser",
    "Browser",
    "Show or hide the browser down the window's left: what to load on the selected track — signal's rig on a guitar or a bass (its profiles, its stacks' patches: a click plays one, a Shift-click switches to it at the cursor).",
    "View",
    "B",
);

/// Toggles asked for on the Action List and not yet carried out.
static ASKED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Ask the window to show or hide the browser ([`BROWSER`]).
pub fn request_toggle() {
    ASKED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// What the browser shows: the selected track, and its rig as the browser
/// lists it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Shown {
    track: String,
    name: String,
    #[cfg(feature = "signal")]
    rig: Option<crate::rig_actions::RigView>,
}

/// The selected track (the first, when several are) and what to show for
/// it — read off the window's thread.
#[cfg(feature = "native")]
fn read() -> Option<Shown> {
    let (daw, project) = crate::actions::song().ok()?;
    let track = daw::service::Tracks::all(daw, project.clone())
        .into_iter()
        .find(|t| t.selected)?;
    Some(Shown {
        #[cfg(feature = "signal")]
        rig: crate::rig_actions::browse(daw, &project, &track.guid),
        track: track.guid,
        name: track.name,
    })
}

/// Run action `id` with `args`, off the window's thread.
#[cfg(feature = "signal")]
fn run(id: &'static str, args: serde_json::Value) {
    #[cfg(feature = "native")]
    if let Some(runtime) = crate::open::runtime() {
        runtime.spawn_blocking(move || {
            if let Err(e) = crate::actions::list().run_json(id, args.to_string().as_bytes()) {
                tracing::warn!(browser.action = id, browser.error = %e, "the browser's press was refused");
            }
        });
    }
    #[cfg(not(feature = "native"))]
    let _ = (id, args);
}

/// Where the cursor is, as an action's `at` (`12.5s`).
#[cfg(feature = "signal")]
fn cursor_at() -> String {
    let at = crate::engine::Transport::shared()
        .map_or(0.0, |t| t.reading().at)
        .max(0.0);
    format!("{at:.3}s")
}

/// A row of the browser's list.
#[cfg(feature = "signal")]
fn row_style(on: bool) -> String {
    let (ink, wash) = if on {
        ("#34d399", "#34d3991f")
    } else {
        ("#e5e7eb", "transparent")
    };
    format!(
        "display:block; width:100%; text-align:left; padding:6px 10px; border:none; \
         border-radius:6px; font-size:12px; cursor:pointer; color:{ink}; background:{wash};"
    )
}

/// A heading in the browser's list.
#[cfg(feature = "signal")]
const HEADING: &str = "padding:10px 10px 4px; font-size:10px; letter-spacing:0.08em; \
     text-transform:uppercase; color:#8b9099;";

/// The browser, down the window's left beside the views, while it is
/// pinned open ([`crate::shell::Pins::browser`], `b`).
#[component]
pub fn BrowserColumn() -> Element {
    // Read every render: `try_use_context` is a hook, and one skipped on
    // some renders panics on the next.
    let pins = try_use_context::<crate::shell::Pins>();
    let open = pins.is_some_and(|pins| (pins.browser)());
    let mut shown = use_signal(|| None::<Shown>);
    // A toggle asked for on the Action List (a model, a menu).
    use_future(move || async move {
        loop {
            if ASKED.swap(0, std::sync::atomic::Ordering::Relaxed) % 2 == 1
                && let Some(pins) = pins
            {
                let mut browser = pins.browser;
                let was = *browser.peek();
                browser.set(!was);
            }
            architect::platform::sleep(std::time::Duration::from_millis(TOGGLE_EVERY_MS)).await;
        }
    });
    use_future(move || async move {
        loop {
            #[cfg(feature = "native")]
            if let Some(runtime) = crate::open::runtime()
                && let Ok(read) = runtime.spawn_blocking(read).await
                && *shown.peek() != read
            {
                shown.set(read);
            }
            architect::platform::sleep(std::time::Duration::from_millis(READ_EVERY_MS)).await;
        }
    });
    if !open {
        return rsx! {};
    }
    let body = shown().map_or_else(
        || {
            rsx! {
                div { style: "padding:14px 10px; font-size:12px; color:{DIM};",
                    "Select a track to see what to load on it."
                }
            }
        },
        |shown| rsx! { Listing { shown } },
    );
    rsx! {
        div {
            style: "position:relative; flex:none; width:280px; overflow-y:auto; \
                    background:{BAR_BG}; border-right:1px solid {RULE};",
            div { style: "padding:12px 10px 6px; font-size:13px; font-weight:600; color:#e5e7eb;",
                "Browser"
            }
            {body}
        }
    }
}

/// What the browser lists for `shown`.
#[component]
fn Listing(shown: Shown) -> Element {
    let name = shown.name.clone();
    #[cfg(feature = "signal")]
    if let Some(rig) = shown.rig.clone() {
        return rsx! {
            div { style: "padding:0 10px 8px; font-size:12px; color:{DIM};", "{name}" }
            RigListing { track: shown.track, rig }
        };
    }
    rsx! {
        div { style: "padding:0 10px 8px; font-size:12px; color:{DIM};", "{name}" }
        div { style: "padding:6px 10px; font-size:12px; color:{DIM};",
            "Nothing to load here yet: signal's rigs are for guitars and basses."
        }
    }
}

/// A guitar's or a bass's rig, to load and to play.
#[cfg(feature = "signal")]
#[component]
fn RigListing(track: String, rig: crate::rig_actions::RigView) -> Element {
    let instrument = rig.instrument.clone();
    let loaded = rig.loaded;
    let profiles = rig.profiles.iter().map(|profile| {
        let on = loaded && *profile == rig.profile;
        let (track, instrument, profile) = (track.clone(), instrument.clone(), profile.clone());
        let label = profile.clone();
        rsx! {
            button {
                key: "{label}",
                style: row_style(on),
                title: "Load this profile on the track",
                onclick: move |_| run(
                    "RIG_LOAD",
                    serde_json::json!({
                        "track": track,
                        "instrument": instrument,
                        "profile": profile,
                    }),
                ),
                "{label}"
            }
        }
    });
    let stacks = rig.stacks.iter().map(|(stack, patches)| {
        let patches = patches.iter().map(|(patch, on)| {
            let (track, patch, on) = (track.clone(), patch.clone(), *on);
            let label = patch.clone();
            rsx! {
                button {
                    key: "{label}",
                    style: row_style(on),
                    title: "Click: play it now. Shift-click: switch to it here in the song.",
                    onclick: move |evt: MouseEvent| {
                        if evt.modifiers().shift() {
                            run(
                                "TONE_INSERT",
                                serde_json::json!({ "track": track, "patch": patch, "at": cursor_at() }),
                            );
                        } else {
                            run("TONE_SET", serde_json::json!({ "track": track, "patch": patch }));
                        }
                    },
                    "{label}"
                }
            }
        });
        rsx! {
            div { key: "{stack}",
                div { style: HEADING, "{stack}" }
                {patches}
            }
        }
    });
    let heading = if loaded {
        "Profiles"
    } else {
        "Load a rig: profiles"
    };
    rsx! {
        div { style: HEADING, "{heading}" }
        {profiles}
        {stacks}
    }
}
