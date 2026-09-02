//! The instrument menu — the app's front door.
//!
//! The catalogue itself (what rigs exist, their slugs, labels and blurbs)
//! lives in [`signal_rigs_proto::Rig`], which the engine also reads for its
//! router scopes — one authority, so a rig cannot be called one thing on the
//! wire and another on screen. What stays here is what is genuinely this
//! binary's business:
//!
//! - [`available`] — which rigs *this build* can open. The phone runs its
//!   rigs in-process and only two of them compile for iOS; the desktop
//!   reaches every one through the engine.
//! - [`icon`] — the rail glyph, an app UI concern.
//! - [`RigMenu`] — the menu itself, rendered by both the desktop workspace
//!   (`rig_view`) and the phone shell (`mobile_view`).
//!
//! Guide is deliberately *not* a rig. It belongs to the session domain
//! (click, count-in, section cues), so the menu pins it above the
//! instruments as a separate row rather than pretending it is an engine.

use dioxus::prelude::*;
pub use signal_rigs_proto::Rig;

use crate::prefs;

/// Whether this rig actually runs in this build — see the module note.
#[cfg(target_os = "ios")]
pub fn available(rig: Rig) -> bool {
    match rig {
        Rig::Guitar => cfg!(feature = "signal-guitar"),
        Rig::Keys => cfg!(feature = "signal-keys-rig"),
        _ => false,
    }
}

/// See the iOS arm above.
#[cfg(not(target_os = "ios"))]
pub fn available(rig: Rig) -> bool {
    match rig {
        // No vocal chain engine exists yet.
        Rig::Vocals => false,
        _ => cfg!(feature = "signal"),
    }
}

/// The rail glyph. `fts_chrome` has no vocal glyph yet, so Vocals borrows the
/// perform icon until one lands in that repo.
pub fn icon(rig: Rig) -> fts_chrome::Icon {
    use fts_chrome::Icon;
    match rig {
        Rig::Guitar => Icon::Guitar,
        Rig::Keys => Icon::Keys,
        Rig::Drums => Icon::Drums,
        Rig::Bass => Icon::Bass,
        Rig::Vocals => Icon::Perform,
        Rig::Synth => Icon::Synth,
        Rig::Ekit => Icon::Drums,
        Rig::Space => Icon::Browser,
    }
}

/// Guide is a session surface, not a rig: click, count-in and spoken section
/// cues.
///
/// It keeps its place in the menu because that is where a player looks for it
/// — you start the click before you pick up an instrument — but this binary is
/// Signal only, and the session domain lives in another repo. So the card is
/// shown and marked plainly as not here yet, which beats omitting it and
/// leaving someone hunting for a thing they remember.
pub const GUIDE_AVAILABLE: bool = false;

// ── Remembering where you were ──────────────────────────────────────────────

/// The rig to open on launch: an explicit request first, then the remembered
/// one. `None` means show the menu — which is what a first-ever launch gets.
pub fn load_last() -> Option<Rig> {
    // An explicit request wins over the remembered rig: `--rig keys` (or
    // `FTS_OPEN_RIG=keys`) is someone saying "open here now", and having it
    // lose to last session's choice would make the flag useless exactly when
    // you reach for it.
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(k) = std::env::var("FTS_OPEN_RIG")
        .ok()
        .and_then(|s| Rig::from_slug(s.trim()))
    {
        return Some(k);
    }
    #[cfg(target_arch = "wasm32")]
    {
        let hash = web_sys::window()
            .and_then(|w| w.location().hash().ok())
            .unwrap_or_default();
        if let Some(rig) = hash.trim_start_matches('#').split('/').nth(1) {
            if let Some(k) = Rig::from_slug(rig) {
                return Some(k);
            }
        }
    }
    // A remembered rig that this build cannot open would strand the app on a
    // dead screen, so it falls back to the menu.
    prefs::get("last-rig")
        .as_deref()
        .and_then(Rig::from_slug)
        .filter(|k| available(*k))
}

pub fn store_last(rig: Option<Rig>) {
    match rig {
        Some(k) => prefs::set("last-rig", k.slug()),
        None => prefs::remove("last-rig"),
    }
}

// ── The menu ────────────────────────────────────────────────────────────────

/// The instrument menu. `phone` swaps the desktop's centred card grid for a
/// single scrolling column of tap targets; the list itself is the same.
#[component]
pub fn RigMenu(on_pick: EventHandler<Rig>, phone: bool) -> Element {
    let (wrap, grid) = if phone {
        (
            "flex: 1; min-height: 0; overflow-y: auto; padding: 18px 18px 24px; \
             display: flex; flex-direction: column; gap: 14px;",
            "display: flex; flex-direction: column; gap: 10px;",
        )
    } else {
        (
            "flex: 1; min-height: 0; overflow-y: auto; padding: 32px 24px; display: flex; \
             flex-direction: column; align-items: center; justify-content: center; gap: 18px;",
            "display: flex; gap: 12px; flex-wrap: wrap; justify-content: center; max-width: 600px;",
        )
    };

    rsx! {
        div { style: "{wrap}",
            // Guide first: on stage you start the click before you pick up an
            // instrument. It reads as a band above the rigs, not one of them.
            GuideCard { phone }
            div { style: "{grid}",
                for kind in Rig::ALL.iter().copied() {
                    RigCard {
                        key: "{kind.slug()}",
                        kind,
                        phone,
                        on_pick: move |_| on_pick.call(kind),
                    }
                }
            }
        }
    }
}

/// One instrument. Available rigs carry the accent edge-LED (the stompbox
/// cue the rig shell uses); the rest read as quietly not-yet.
#[component]
fn RigCard(kind: Rig, phone: bool, on_pick: EventHandler<()>) -> Element {
    let size = if phone {
        "width: 100%; padding: 16px 16px 16px 20px;"
    } else {
        "width: 178px; padding: 14px 14px 14px 18px;"
    };
    let title = if phone { "18px" } else { "14px" };

    if available(kind) {
        rsx! {
            button {
                style: "position: relative; overflow: hidden; text-align: left; border: none; \
                        border-radius: 12px; {size} cursor: pointer; \
                        background: linear-gradient(135deg, #10283f, #0b3a52); color: #e0f2fe; \
                        display: flex; flex-direction: column; gap: 5px;",
                onclick: move |_| on_pick.call(()),
                span {
                    style: "position: absolute; left: 0; top: 12px; bottom: 12px; width: 4px; \
                            border-radius: 0 2px 2px 0; background: #38bdf8; box-shadow: 0 0 12px #38bdf8;",
                }
                span { style: "font-size: {title}; font-weight: 700;", "{kind.label()}" }
                span { style: "font-size: 11px; opacity: 0.72;", "{kind.blurb()}" }
            }
        }
    } else {
        rsx! {
            div {
                style: "border: 1px solid #1f1f23; border-radius: 12px; {size} \
                        background: #131316; color: #52525b; display: flex; flex-direction: column; gap: 4px;",
                div { style: "display: flex; align-items: center; gap: 8px;",
                    span { style: "font-size: {title}; font-weight: 700;", "{kind.label()}" }
                    span { style: "margin-left: auto; font-size: 9px; font-weight: 600; \
                                   letter-spacing: 0.12em; color: #3f3f46;", "SOON" }
                }
                span { style: "font-size: 11px;", "{kind.blurb()}" }
            }
        }
    }
}

/// The Guide band — session's click / count-in / section cues.
#[component]
fn GuideCard(phone: bool) -> Element {
    let size = if phone {
        "width: 100%; padding: 14px 16px;"
    } else {
        "width: 100%; max-width: 600px; padding: 14px 18px;"
    };
    let (bg, fg, meta) = if GUIDE_AVAILABLE {
        ("#132318", "#dcfce7", "#4ade80")
    } else {
        ("#131316", "#52525b", "#3f3f46")
    };
    rsx! {
        div {
            style: "border: 1px solid #1f1f23; border-radius: 12px; {size} background: {bg}; \
                    color: {fg}; display: flex; align-items: center; gap: 12px;",
            div { style: "display: flex; flex-direction: column; gap: 3px; min-width: 0;",
                span { style: "font-size: 11px; font-weight: 600; letter-spacing: 0.14em; \
                               text-transform: uppercase; color: {meta};", "Session" }
                span { style: "font-size: 15px; font-weight: 700;", "Guide" }
                span { style: "font-size: 11px; opacity: 0.72;", "click, count-in & section cues" }
            }
            if !GUIDE_AVAILABLE {
                span { style: "margin-left: auto; font-size: 9px; font-weight: 600; \
                               letter-spacing: 0.12em; color: {meta};", "SOON" }
            }
        }
    }
}
