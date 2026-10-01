//! The iPhone shell, on Blitz.
//!
//! The app ships as **Signal**, so its front door is the instrument menu
//! (`crate::rigs::RigMenu`) — the same catalogue the desktop renders. A
//! remembered rig opens straight into itself on launch; the menu is one tap
//! back (the rig's rail has a Rigs button).
//!
//! The guitar rig is `GuitarRigRemote`, the surface the desktop and the
//! Android remote mount: on a phone held sideways it lays the chain out a
//! page at a time in frame's phone faces (`signal_guitar_ui`'s `phone`), so
//! the phone has no rig UI of its own. This shell only gives it what a phone
//! window can tell it — its size in points (the form factor) and which side
//! the camera housing is on — and the way back to the menu.
//!
//! Blitz draws it (dioxus-native, the desktop's renderer): the faces are
//! painted custom widgets, which the WebView this replaced could not host.
//! The rig clients come from `rig_engine.rs` (in-process `LocalServer`) and
//! are provided as context, so every shared component works unchanged.

#[cfg(feature = "signal-keys-rig")]
use crate::keys_view;
use dioxus::prelude::*;
use signal_guitar_ui::{GuitarRigRemote, IslandLeft, PhoneHost, WindowAspect, WindowSize};

use crate::rigs::{Rig, RigMenu};

const SIGNAL_TAILWIND: &str = include_str!("../assets/tailwind-signal.css");

/// Blitz gives `body` an 8px margin, and the root's background is also what
/// fills under the status bar and the home indicator.
const ROOT_CSS: &str = "html, body { margin: 0; padding: 0; background: #0f1012; }";

/// Which top-level screen is showing.
#[derive(Clone, Copy, PartialEq)]
enum MobileScreen {
    /// The instrument menu — the front door.
    Menu,
    /// The guitar rig.
    Rig,
    /// The keys rig (sampler engine + downloaded packs).
    #[cfg(feature = "signal-keys-rig")]
    Keys,
}

impl MobileScreen {
    /// The screen a rig opens onto. A rig with no phone surface keeps you on
    /// the menu rather than opening a blank one — the menu already marks it
    /// as not-yet, so nothing needs saying twice.
    fn for_rig(rig: Rig) -> MobileScreen {
        match rig {
            Rig::Guitar => MobileScreen::Rig,
            #[cfg(feature = "signal-keys-rig")]
            Rig::Keys => MobileScreen::Keys,
            _ => MobileScreen::Menu,
        }
    }
}

/// The phone app root: hand winit's window to the app's scene, measure the
/// window, bootstrap the engine into context, then route between the
/// instrument menu and the rigs.
#[component]
pub fn MobileApp() -> Element {
    // First, before anything else: winit's window joins the app's window
    // scene (`ios_scene`) — without one iOS 27 never shows it.
    let window = dioxus_native::use_window();
    use_hook({
        let window = window.clone();
        move || {
            use dioxus_native::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(handle) = window.window_handle()
                && let RawWindowHandle::UiKit(uikit) = handle.as_raw()
            {
                crate::ios_scene::window_created(uikit.ui_view);
            }
        }
    });

    // The window's size in points, its aspect, and the housing's side: the
    // rig's form factor and where its page keeps clear. Read on a timer
    // (as the desktop's rig view does) — a rotation changes all three.
    let mut size = use_context_provider(|| WindowSize(Signal::new((402.0, 874.0))));
    let mut aspect = use_context_provider(|| WindowAspect(Signal::new(402.0 / 874.0)));
    let mut island = use_context_provider(|| IslandLeft(Signal::new(false)));
    use_future(move || {
        let window = window.clone();
        async move {
            loop {
                let px = window.surface_size();
                if px.height > 0 {
                    let scale = window.scale_factor().max(0.1);
                    let pt = (f64::from(px.width) / scale, f64::from(px.height) / scale);
                    let was = *size.0.peek();
                    if (was.0 - pt.0).abs() > 1.0 || (was.1 - pt.1).abs() > 1.0 {
                        size.0.set(pt);
                        aspect.0.set(pt.0 / pt.1);
                    }
                }
                let left = crate::ios_scene::island_on_left() == Some(true);
                if *island.0.peek() != left {
                    island.0.set(left);
                }
                architect::platform::sleep(std::time::Duration::from_millis(300)).await;
            }
        }
    });

    let engine = crate::rig_engine::engine();
    rsx! {
        document::Style { {ROOT_CSS} }
        document::Style { {SIGNAL_TAILWIND} }
        // The viewport, sized explicitly: an absolute box stretched only by
        // its insets gets no height in Blitz. Blitz keeps the safe area's
        // top and bottom out of the viewport; the sides are the page's own
        // (`BLITZ_SAFE_AREA_SIDES=0`, so the rig runs under the housing).
        div {
            style: "position: absolute; top: 0; left: 0; width: 100vw; height: 100vh; \
                    box-sizing: border-box; display: flex; flex-direction: column; \
                    background: #0f1012; color: #e4e4e7; overflow: hidden; \
                    font-family: Inter, -apple-system, sans-serif;",
            match engine {
                Some(engine) => {
                    let _ = provide_context(engine.rig.clone());
                    let _ = provide_context(engine.stream.clone());
                    let _ = provide_context(engine.settings.clone());
                    #[cfg(feature = "signal-keys-rig")]
                    let _ = provide_context(engine.keys.clone());
                    rsx! { Router {} }
                }
                None => rsx! {
                    div { style: "display: flex; flex: 1; align-items: center; justify-content: center; flex-direction: column; gap: 8px;",
                        span { style: "width: 12px; height: 12px; border-radius: 999px; background: #ef4444;" }
                        span { style: "font-size: 14px; font-weight: 600;", "Engine failed to start" }
                        span { style: "font-size: 12px; color: #71717a;", "Check the logs (audio device / library load)." }
                    }
                },
            }
        }
    }
}

/// Menu → rig. Each screen owns its orientation (set on mount), so
/// navigating swaps the component and rotates the phone.
#[component]
fn Router() -> Element {
    // Launch back into the rig you were playing; the menu is the cold-start
    // and the one-tap-back screen.
    let mut screen = use_signal(|| {
        crate::rigs::load_last()
            .map(MobileScreen::for_rig)
            .unwrap_or(MobileScreen::Menu)
    });
    let to_menu = move |()| {
        crate::rigs::store_last(None);
        screen.set(MobileScreen::Menu);
    };
    match screen() {
        MobileScreen::Menu => rsx! {
            MenuPage {
                on_pick: move |rig: Rig| {
                    crate::rigs::store_last(Some(rig));
                    screen.set(MobileScreen::for_rig(rig));
                },
            }
        },
        MobileScreen::Rig => rsx! {
            GuitarPage { on_home: to_menu }
        },
        #[cfg(feature = "signal-keys-rig")]
        MobileScreen::Keys => rsx! {
            keys_view::KeysShell { on_home: to_menu }
        },
    }
}

/// The front door: the shared instrument menu, portrait, phone layout.
#[component]
fn MenuPage(on_pick: EventHandler<Rig>) -> Element {
    use_hook(crate::ios_orientation::portrait);
    rsx! {
        div { style: "flex: 1; min-height: 0; display: flex; flex-direction: column; overflow-y: auto;",
            RigMenu { phone: true, on_pick }
        }
    }
}

/// The guitar rig, held sideways: the shared remote, which lays itself out
/// for the phone (frame's phone faces, a page at a time). Its rail's Rigs
/// button comes back here.
#[component]
fn GuitarPage(on_home: EventHandler<()>) -> Element {
    use_hook(crate::ios_orientation::landscape);
    use_context_provider(|| PhoneHost {
        on_home: Callback::new(move |()| on_home.call(())),
    });
    rsx! {
        div { style: "flex: 1; min-width: 0; min-height: 0; display: flex; flex-direction: column;",
            GuitarRigRemote {}
        }
    }
}
