//! The iPad layout's pieces as panels of a host's dock: the footswitch
//! grid ([`Switches`]) and the tone browser ([`Tones`]), each laid out by
//! the box it is given, and [`RigProviders`], which a host draws once
//! around every such panel so they share one rig — its view state, its
//! performance state, the browser's pick and focus, the drags and the
//! popups.
//!
//! The rig's vox clients (`RigClient`, `RigStreamClient`, the perform
//! clients, the TONE3000 pair) come from context, from the host's
//! connection; a panel mounted before they are there says it is waiting.

use dioxus::prelude::*;

use super::switches::{SwitchSize, TouchSwitches};
use crate::proto::rig::RigClient;
use crate::ui::state::{RigViewState, use_rig_state};
use signal_rig_perform_ui::{BrowserFocus, BuildPick};

/// Everything the rig's panels read, made once for all of them. Draws
/// `children` filling its parent, with the rig's popup layer over them.
/// Mount it under the rig's clients (keyed by the connection, so a new
/// connection makes it afresh).
#[component]
pub fn RigProviders(children: Element) -> Element {
    // Drags that outlive their control (Blitz has no pointer capture),
    // menus drawn over everything, and whether a finger plays the rig.
    let drag_bus = signal_widgets::DragBus::provide();
    signal_widgets::PopupHost::provide();
    let touch = signal_widgets::Touch::provide();
    let rig = use_hook(try_consume_context::<RigClient>);
    crate::ui::param_writer::use_param_writer(rig);
    let state = use_rig_state();
    use_context_provider(|| state);
    use_context_provider(|| crate::ui::frame_surface::FacesMove(state.running));
    // The shared performance state (the setlist, the sidebar views).
    let perf = signal_rig_perform_ui::use_perform();
    let faces = signal_rig_perform_ui::sidebar_views::use_compositions(perf);
    use_context_provider(|| crate::ui::face_chrome::FacePresets(faces));
    use_context_provider(|| BuildPick {
        part: Signal::new(None),
        building: Signal::new(false),
    });
    use_context_provider(|| BrowserFocus(Signal::new(None)));
    rsx! {
        fts_audio_ui::drag::DragProvider { fill: true,
            div {
                style: "position: relative; width: 100%; height: 100%; min-width: 0; min-height: 0; display: flex;",
                onpointermove: move |e: PointerEvent| drag_bus.root_move(&e),
                onpointerup: move |_| drag_bus.root_up(),
                onpointercancel: move |_| drag_bus.root_up(),
                onpointerdown: move |e: PointerEvent| touch.observe(&e),
                {children}
                signal_widgets::PopupLayer {}
                document::Style { {crate::ui::theme::CSS} }
            }
        }
    }
}

/// A rig panel's waiting state: no rig connection above it yet.
#[component]
pub fn Connecting() -> Element {
    rsx! {
        div {
            style: "flex: 1; min-width: 0; min-height: 0; display: flex; align-items: center; \
                    justify-content: center; padding: 12px; color: #8e8e98; font-size: 13px; \
                    font-family: Inter, -apple-system, sans-serif; text-align: center;",
            "Connecting to the rig engine…"
        }
    }
}

/// The rig's view state from [`RigProviders`], when the rig is connected.
/// Both reads are hooks, so both always run.
fn connected() -> Option<RigViewState> {
    let rig = try_use_context::<RigClient>();
    let state = try_use_context::<RigViewState>();
    rig.and(state)
}

/// The footswitch scenes: the ten switches (the hold layer over the
/// switches under the feet), filling the panel.
#[component]
pub fn Switches() -> Element {
    let Some(state) = connected() else {
        return rsx! { Connecting {} };
    };
    let perf = state.perf.read().clone();
    rsx! {
        div {
            style: "flex: 1; min-width: 0; min-height: 0; display: flex; flex-direction: column; \
                    padding: 8px; box-sizing: border-box; overflow: hidden; background: #0f0f12;",
            TouchSwitches { perf, size: SwitchSize::Gig, fill: true }
        }
    }
}

/// The rig's tone browser: its kinds, their presets, captures and
/// TONE3000 — narrow (one step at a time) in a narrow panel.
#[component]
pub fn Tones() -> Element {
    let Some(state) = connected() else {
        return rsx! { Connecting {} };
    };
    rsx! {
        div {
            style: "flex: 1; min-width: 0; min-height: 0; display: flex; flex-direction: column; \
                    overflow: hidden; background: #0f0f12;",
            super::browser::Browser { state }
        }
    }
}
