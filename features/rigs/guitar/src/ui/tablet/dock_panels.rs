//! The iPad layout's pieces as panels of a host's dock, each laid out by
//! the box it is given: the footswitch grid ([`Switches`]), the tone
//! browser ([`Tones`]), the sidebar ([`Sidebar`]), the routing grid
//! ([`Grid`]), the FX row ([`Fx`]), Edit ([`Edit`]), the macro bar
//! ([`Macros`]), the rig's status ([`Status`]), the tuner ([`Tuner`]), the
//! DI player ([`DiPlayer`]) and Setup ([`Setup`]); and [`RigProviders`],
//! which a host draws around such panels (once around all of them, or
//! once around each) so they share one rig — its view state, its
//! performance state, the routing's selection, the browser's pick and
//! focus, the drags and the popups.
//!
//! A host whose panels are apart (a pane each, [`RigProviders`] around
//! each) keeps what they share in step itself: the selection
//! ([`RoutingSel`]), the browser's focus ([`BrowserFocus`]) and the part
//! being built ([`BuildPick`]) are signals in context, there to be read
//! and set. What a panel asks of its host (bring the browser up, show
//! Setup) goes through [`PanelHost`], when the host provides one.
//!
//! The rig's vox clients (`RigClient`, `RigStreamClient`, the perform
//! clients, the TONE3000 pair) come from context, from the host's
//! connection; a panel mounted before they are there says it is waiting.

use dioxus::prelude::*;

use super::switches::{SwitchSize, TouchSwitches};
use crate::proto::rig::RigClient;
use crate::ui::state::{RigViewState, use_rig_state};
use signal_rig_perform_ui::{setlist, sidebar_views};

pub use super::routing::{RoutingSel, Selected};
pub use super::setup::SetupTab;
pub use signal_rig_perform_ui::{BrowserFocus, BuildPick, Focus};

/// What a panel asks of the host that placed it. Optional context: with
/// none, the asks are dropped (the panel still works on its own).
#[derive(Clone, Copy)]
pub struct PanelHost {
    /// Bring the browser up: a part's "Patch…", a face's preset name. Its
    /// focus ([`BrowserFocus`]) is already set.
    pub open_browser: Callback<()>,
    /// Show Setup on a tab (`guitar`, `audio`, `midi`): the status bar's
    /// MIDI and audio lights.
    pub open_setup: Callback<&'static str>,
}

/// The browser's kind for a block type: a drive slot's pedals are the
/// Drive module's presets; a module's its own; any other block its type's.
#[must_use]
pub fn browser_kind(block_type: &str) -> String {
    let t = block_type.to_lowercase();
    if t.starts_with("module:") {
        block_type.to_string()
    } else if t == "drive" || t == "boost" {
        "module:Drive".to_string()
    } else {
        format!("block:{t}")
    }
}

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
    let focus = use_context_provider(|| BrowserFocus(Signal::new(None)));
    // The routing's selection (the FX row's block, Edit's).
    use_context_provider(|| RoutingSel(Signal::new(None)));
    let host = try_use_context::<PanelHost>();
    // A face's preset name: the browser, on that block's presets.
    use_context_provider(|| {
        crate::ui::face_chrome::OpenPresets(Callback::new(
            move |(_block, block_type): (String, String)| {
                let mut f = focus.0;
                f.set(Some(Focus {
                    kind: browser_kind(&block_type),
                    preset: String::new(),
                    variation: String::new(),
                    search: false,
                }));
                if let Some(h) = host {
                    h.open_browser.call(());
                }
            },
        ))
    });
    // "Patch…" on a section or part, or the presets' search.
    use_context_provider(|| setlist::PickPart {
        open: Callback::new(move |()| {
            if let Some(h) = host {
                h.open_browser.call(());
            }
        }),
    });
    // A press outside the macro bar and its panel closes the panel.
    let away = use_context_provider(super::macros::MacroAway::new);
    rsx! {
        fts_audio_ui::drag::DragProvider { fill: true,
            div {
                style: "position: relative; width: 100%; height: 100%; min-width: 0; min-height: 0; display: flex;",
                onpointermove: move |e: PointerEvent| drag_bus.root_move(&e),
                onpointerup: move |_| drag_bus.root_up(),
                onpointercancel: move |_| drag_bus.root_up(),
                onpointerdown: move |e: PointerEvent| {
                    touch.observe(&e);
                    away.press();
                },
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

/// A panel's box: filling, a column, on the rig's desk.
const BOX: &str = "flex: 1; min-width: 0; min-height: 0; display: flex; flex-direction: column; \
                   overflow: hidden; background: #0f0f12;";

/// The sidebar for the footswitch mode: the set (its songs, their
/// sections, the stacks in them), the profile's stacks, or the presets.
#[component]
pub fn Sidebar() -> Element {
    let perf = try_use_context::<signal_rig_perform_ui::PerfState>();
    let (Some(state), Some(perf)) = (connected(), perf) else {
        return rsx! { Connecting {} };
    };
    let mode = state.perf.read().perform_mode;
    rsx! {
        div { style: BOX,
            match mode {
                0 => rsx! { sidebar_views::PresetView { state: perf } },
                1 => rsx! { sidebar_views::ProfileView { state: perf } },
                _ => rsx! { setlist::TabletSetlist { state: perf } },
            }
        }
    }
}

/// The routing grid: the patch playing, its modules and blocks. A tap
/// selects a block or a module ([`RoutingSel`]).
#[component]
pub fn Grid() -> Element {
    let Some(state) = connected() else {
        return rsx! { Connecting {} };
    };
    rsx! {
        div { style: BOX,
            div { style: "flex: 1; min-height: 0;", super::routing::Routing { state } }
        }
    }
}

/// The FX row: the selected block's faces, and under them its preset bar.
#[component]
pub fn Fx() -> Element {
    let Some(state) = connected() else {
        return rsx! { Connecting {} };
    };
    rsx! {
        div { style: BOX,
            div { style: "flex: 1; min-height: 0; display: flex; flex-direction: column;",
                super::fx_row::FxRow { state }
            }
            div { style: "flex-shrink: 0; display: flex; align-items: center; min-height: 52px; \
                          border-top: 1px solid #000;",
                super::fx_row::FxPresetBar { state }
            }
        }
    }
}

/// Edit: a section's changes over the routing and the FX row, to save or
/// discard.
#[component]
pub fn Edit() -> Element {
    let Some(state) = connected() else {
        return rsx! { Connecting {} };
    };
    let perf = state.perf.read().clone();
    rsx! {
        div { style: BOX, super::edit::EditView { state, perf } }
    }
}

/// The macro bar of the patch playing; a knob's panel drops down from it.
#[component]
pub fn Macros() -> Element {
    let Some(state) = connected() else {
        return rsx! { Connecting {} };
    };
    rsx! {
        div { style: "flex: 1; min-width: 0; min-height: 0; display: flex; flex-direction: column; \
                      position: relative; background: #0f0f12;",
            div { style: "flex-shrink: 0; position: relative; z-index: 4; border-bottom: 1px solid #000;",
                super::macros::TouchMacroBar { state }
            }
        }
    }
}

/// The rig's status: the footswitch mode, the DI player, Panic, the MIDI
/// and audio lights, CPU, the IN / OUT / PHONES meters and Mute.
#[component]
pub fn Status() -> Element {
    let Some(state) = connected() else {
        return rsx! { Connecting {} };
    };
    let model = state.perf.read().clone();
    let host = try_use_context::<PanelHost>();
    rsx! {
        div { style: "flex: 1; min-width: 0; min-height: 0; display: flex; flex-direction: column; \
                      justify-content: center; background: #0f0f12;",
            super::topbar::TopBar {
                model,
                state,
                sidebar: false,
                on_sidebar: move |()| {},
                on_setup: move |tab: &'static str| {
                    if let Some(h) = host {
                        h.open_setup.call(tab);
                    }
                },
            }
        }
    }
}

/// The tuner, filling the panel.
#[component]
pub fn Tuner() -> Element {
    if connected().is_none() {
        return rsx! { Connecting {} };
    }
    rsx! {
        div { style: "position: relative; flex: 1; min-width: 0; min-height: 0; display: flex; \
                      overflow: hidden; background: #0f0f12;",
            crate::ui::perform::TunerOverlay { on_close: move |()| {} }
        }
    }
}

/// The DI player: the recorded takes, played through the rig in place of
/// the input, and its loop recorder.
#[component]
pub fn DiPlayer() -> Element {
    if connected().is_none() {
        return rsx! { Connecting {} };
    }
    rsx! {
        div { style: "position: relative; flex: 1; min-width: 0; min-height: 0; display: flex; \
                      align-items: flex-start; padding: 8px; background: #0f0f12;",
            super::player::Player {}
        }
    }
}

/// Setup on `tab`: the guitar (its tone, gates and level), the audio
/// device and routing, the MIDI devices and mappings.
#[component]
pub fn Setup(tab: SetupTab) -> Element {
    let Some(state) = connected() else {
        return rsx! { Connecting {} };
    };
    let shown = use_signal(|| tab);
    rsx! {
        div { style: BOX, super::setup::SetupView { state, tab: shown } }
    }
}
