//! The DAW view's main toolbar: the corner left of the ruler, above the
//! track panel — where REAPER keeps its main toolbar.
//!
//! The toggles are REAPER's main-toolbar set: metronome, auto crossfade,
//! item grouping, ripple per track, grid lines, snapping, locking. The
//! editing ones switch [`crate::options`]; the metronome is the session's
//! own click — the generated Click track, whose mute it flips — because the
//! FTS click is the guide's, not the DAW's built-in metronome.
//!
//! The icons are drawn here as SVG rather than taken from REAPER's theme:
//! that art is REAPER's, and this is a GPL tree.

use dioxus::prelude::*;

use crate::options;

const ON_BG: &str = "#1f5f55";
const ON_FG: &str = "#e8fff9";
const OFF_FG: &str = "#9aa0a8";
const HOVER_RULE: &str = "#3a3f47";

/// Which toggle a button is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Toggle {
    /// The mixer under the arrangement: `x` on a keyboard, and only in the
    /// menu, for a screen without one.
    Mixer,
    Compact,
    Follow,
    Metronome,
    AutoCrossfade,
    Grouping,
    Ripple,
    Grid,
    Snap,
    Lock,
}

impl Toggle {
    const ROWS: [&'static [Self]; 2] = [
        &[
            Self::Metronome,
            Self::AutoCrossfade,
            Self::Grouping,
            Self::Ripple,
        ],
        &[
            Self::Grid,
            Self::Snap,
            Self::Lock,
            Self::Follow,
            Self::Compact,
        ],
    ];

    const fn title(self) -> &'static str {
        match self {
            Self::Metronome => "Metronome (the Click track)",
            Self::AutoCrossfade => "Auto crossfade",
            Self::Grouping => "Item grouping",
            Self::Ripple => "Ripple editing per track",
            Self::Grid => "Grid lines",
            Self::Snap => "Snapping",
            Self::Lock => "Locking",
            Self::Compact => "Compact track panel — more room for the arrangement",
            Self::Mixer => "The mixer (x)",
            Self::Follow => "Follow the play cursor — page the view to wherever it goes",
        }
    }

    /// The name the menu gives it, beside its icon.
    const fn label(self) -> &'static str {
        match self {
            Self::Metronome => "Metronome",
            Self::AutoCrossfade => "Auto crossfade",
            Self::Grouping => "Item grouping",
            Self::Ripple => "Ripple per track",
            Self::Grid => "Grid lines",
            Self::Snap => "Snap",
            Self::Lock => "Lock",
            Self::Follow => "Follow the play cursor",
            Self::Compact => "Compact track panel",
            Self::Mixer => "Mixer",
        }
    }

    const fn option(self) -> Option<&'static options::Option> {
        match self {
            // These are the window's, not the session's.
            Self::Compact | Self::Follow | Self::Metronome | Self::Mixer => None,
            Self::AutoCrossfade => Some(&options::AUTO_CROSSFADE),
            Self::Grouping => Some(&options::GROUPING),
            Self::Ripple => Some(&options::RIPPLE),
            Self::Grid => Some(&options::GRID),
            Self::Snap => Some(&options::SNAP),
            Self::Lock => Some(&options::LOCKING),
        }
    }

    /// The icon's strokes, on a 24-unit square.
    const fn paths(self) -> &'static [&'static str] {
        match self {
            // A metronome: the body and the arm.
            Self::Metronome => &[
                "M8 21 L10.5 3 H13.5 L16 21 Z",
                "M12 16 L17.5 6.5",
                "M6 21 H18",
            ],
            // Two fades crossing.
            Self::AutoCrossfade => &["M3 19 C9 19 15 5 21 5", "M3 5 C9 5 15 19 21 19"],
            // Two links of a chain.
            Self::Grouping => &[
                "M10 14 L8.5 15.5 A3.5 3.5 0 0 1 3.5 10.5 L6.5 7.5 A3.5 3.5 0 0 1 11 7",
                "M14 10 L15.5 8.5 A3.5 3.5 0 0 1 20.5 13.5 L17.5 16.5 A3.5 3.5 0 0 1 13 17",
                "M9 15 L15 9",
            ],
            // An item pushing the ones after it along one track.
            Self::Ripple => &["M3 9 H9 V15 H3 Z", "M12 12 H20", "M17 9 L20 12 L17 15"],
            // Grid lines.
            Self::Grid => &[
                "M4 4 V20",
                "M10 4 V20",
                "M16 4 V20",
                "M4 8 H20",
                "M4 16 H20",
            ],
            // A magnet.
            Self::Snap => &[
                "M6 4 V11 A6 6 0 0 0 18 11 V4",
                "M6 8 H10",
                "M14 8 H18",
                "M10 4 V11 A2 2 0 0 0 14 11 V4",
            ],
            // A padlock.
            Self::Lock => &["M6 11 H18 V20 H6 Z", "M8.5 11 V8 A3.5 3.5 0 0 1 15.5 8 V11"],
            // The play cursor, and the page turning to keep it in view:
            // the line, and an arrow along to the right of it.
            Self::Follow => &["M7 4 V20", "M11 12 H19", "M15.5 8.5 L19 12 L15.5 15.5"],
            // Three faders: their grooves and their caps.
            Self::Mixer => &[
                "M6 4 V20",
                "M12 4 V20",
                "M18 4 V20",
                "M4 14 H8",
                "M10 8 H14",
                "M16 12 H20",
            ],
            // A panel squeezed from the right: its edge, and an arrow
            // pushing into it.
            Self::Compact => &[
                "M4 4 V20",
                "M20 4 V20",
                "M9 12 H16",
                "M12.5 8.5 L9 12 L12.5 15.5",
            ],
        }
    }
}

/// The toolbar. `click` is the generated Click track's guid, if the session
/// has one; `edits` is the arrangement's queue, which carries the mute.
#[component]
pub fn MainToolbar(
    click: Option<String>,
    click_muted: bool,
    edits: crate::studio::Edits,
    width: f64,
    /// The panel's shape, shared with the arrangement widget.
    compact: std::rc::Rc<std::cell::Cell<bool>>,
    /// The same, as the signal the chrome over the panel re-renders on.
    shape: Signal<bool>,
    /// Whether the view follows the play cursor, shared with the panel
    /// that pages it (see `panel::ArrangementPanel::frame_tick`).
    follow: std::rc::Rc<std::cell::Cell<bool>>,
    /// Touch mode: the toggles in a menu, at a finger's size.
    #[props(default)]
    touch: bool,
    /// Whether the mixer under the arrangement is open, when the view has
    /// one: a toggle in the menu.
    #[props(default)]
    mixer: Option<Signal<bool>>,
) -> Element {
    // Each option's state as last set here; the options themselves are the
    // truth, this is what re-renders the button.
    let mut shown = use_signal(|| {
        Toggle::ROWS
            .iter()
            .flat_map(|row| row.iter())
            .map(|t| (*t, t.option().is_none_or(options::Option::get)))
            .collect::<Vec<_>>()
    });
    let mut metronome = use_signal(|| !click_muted);
    let pins = try_use_context::<crate::shell::Pins>();
    // The window's one way to open the mixer, when it has one.
    let switch = try_use_context::<crate::shell::MixerToggle>();
    let mut following = use_signal(|| follow.get());
    let mut open = use_signal(|| false);
    let is_on = move |t: Toggle| match t {
        Toggle::Metronome => metronome(),
        Toggle::Compact => shape(),
        Toggle::Follow => following(),
        Toggle::Mixer => switch.map_or_else(|| mixer.is_some_and(|open| open()), |s| s.showing()),
        // The lock is shown in the bottom bar too: its signal, when the
        // shell has one, is the one both re-render on.
        Toggle::Lock if pins.is_some() => pins.is_some_and(|p| (p.lock)()),
        other => shown.read().iter().any(|(each, on)| *each == other && *on),
    };
    // What a toggle does, whether pressed in the row or in the menu.
    let press = use_callback(move |toggle: Toggle| match toggle {
        Toggle::Mixer => {
            if let Some(switch) = switch {
                switch.press();
            } else if let Some(mut open) = mixer {
                open.toggle();
            }
        }
        Toggle::Metronome => {
            if let Some(guid) = click.clone() {
                edits
                    .0
                    .borrow_mut()
                    .push(crate::engine::Edit::ToggleMute(guid));
                metronome.toggle();
            }
        }
        Toggle::Follow => {
            let now = !follow.get();
            follow.set(now);
            following.set(now);
        }
        Toggle::Compact => {
            let now = !compact.get();
            compact.set(now);
            let mut shape = shape;
            shape.set(now);
        }
        other => {
            if let Some(option) = other.option() {
                let now = option.toggle();
                if other == Toggle::Grouping {
                    crate::options::grouping_changed(now);
                }
                if other == Toggle::Lock
                    && let Some(mut lock) = pins.map(|p| p.lock)
                {
                    lock.set(now);
                }
                for (each, on) in shown.write().iter_mut() {
                    if *each == other {
                        *on = now;
                    }
                }
            }
        }
    });
    // A finger, or a corner too narrow for the rows (the compact panel's,
    // a phone's): one button, and the toggles in a menu under it. Squeezed
    // into the corner, the rows were cut off.
    if touch || width < FULL_W {
        return rsx! {
            div {
                style: "position:absolute; left:0; top:0; padding:6px 8px; z-index:20;",
                button {
                    title: "Arrangement options",
                    style: "width:44px; height:36px; padding:0; display:flex; align-items:center; \
                            justify-content:center; border-radius:8px; border:1px solid {HOVER_RULE}; \
                            background:#1c1e22; color:{OFF_FG}; cursor:pointer;",
                    onclick: move |_| open.toggle(),
                    lucide_dioxus::Menu { size: 20, color: "currentColor" }
                }
                if open() {
                    // A press outside closes it.
                    div {
                        style: "position:fixed; top:0; left:0; width:100vw; height:100vh; z-index:21;",
                        onclick: move |_| open.set(false),
                    }
                    div {
                        style: "position:absolute; top:48px; left:8px; z-index:22; width:260px; \
                                display:flex; flex-direction:column; gap:2px; padding:6px; \
                                border-radius:12px; background:#1c1e22; border:1px solid {HOVER_RULE}; \
                                box-shadow:0 12px 32px rgba(0,0,0,0.6);",
                        for toggle in mixer
                            .map(|_| Toggle::Mixer)
                            .into_iter()
                            .chain(Toggle::ROWS.iter().flat_map(|row| row.iter()).copied())
                        {
                            button {
                                key: "{toggle:?}",
                                style: menu_row_style(is_on(toggle)),
                                onclick: move |_| press.call(toggle),
                                Icon { toggle, on: is_on(toggle) }
                                span { style: "flex:1; text-align:left;", "{toggle.label()}" }
                                span {
                                    style: "font-size:12px; color:{OFF_FG};",
                                    if is_on(toggle) { "On" } else { "Off" }
                                }
                            }
                        }
                    }
                }
            }
        };
    }
    rsx! {
        div {
            style: "position:absolute; left:0; top:0; width:{width}px; padding:6px 8px; \
                    display:flex; flex-direction:column; gap:4px;",
            for row in Toggle::ROWS {
                div {
                    style: "display:flex; gap:4px;",
                    for toggle in row.iter().copied() {
                        button {
                            key: "{toggle:?}",
                            title: toggle.title(),
                            style: button_style(is_on(toggle)),
                            onclick: move |_| press.call(toggle),
                            Icon { toggle, on: is_on(toggle) }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn Icon(toggle: Toggle, on: bool) -> Element {
    let color = if on { ON_FG } else { OFF_FG };
    rsx! {
        svg {
            width: "18",
            height: "18",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: color,
            stroke_width: "1.8",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            for d in toggle.paths().iter().copied() {
                path { d }
            }
        }
    }
}

/// How wide the corner must be for the rows: the longer row's five 30px
/// buttons, their gaps and the padding.
const FULL_W: f64 = 5.0 * 30.0 + 4.0 * 4.0 + 16.0;

fn menu_row_style(on: bool) -> String {
    let (bg, fg) = if on {
        (ON_BG, ON_FG)
    } else {
        ("transparent", "#d1d5db")
    };
    format!(
        "display:flex; align-items:center; gap:12px; height:44px; padding:0 12px; border:none; \
         border-radius:8px; background:{bg}; color:{fg}; font-family:inherit; font-size:15px; \
         cursor:pointer;"
    )
}

fn button_style(on: bool) -> String {
    let (bg, rule) = if on {
        (ON_BG, ON_BG)
    } else {
        ("transparent", HOVER_RULE)
    };
    format!(
        "flex:0 1 30px; min-width:0; height:26px; padding:0; display:flex; align-items:center; \
         justify-content:center; border-radius:4px; border:1px solid {rule}; background:{bg};"
    )
}
