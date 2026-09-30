//! **Switches** — the footswitch vocabulary every rig's perform strip
//! shares: the tap/hold button, the tile's number and lit/dark colours, the
//! Full / Compact / Hidden toggle in the rig header, the right-click menu's
//! frame and rows, and the MIDI learn rows and badge.
//!
//! A rig composes its own grid out of these (the guitar rig's stacks rotate
//! patches and step through songs; the keys rig's recall mixer scenes), so
//! the pieces take plain values and callbacks — no rig's client, no rig's
//! model. MIDI learn is the rig's too (`signal_rig_host::midi_learn` on the
//! engine side); the rows here only show it and ask for it.

use std::time::Duration;

use dioxus::dioxus_core::Task;
use dioxus::prelude::*;

use crate::PopupHost;

/// A press held this long is a hold, not a tap.
pub const HOLD_MS: u64 = 500;

/// The ring around a lit switch.
pub const LIT_RING: &str =
    "box-shadow: 0 0 0 2px rgba(255,255,255,0.8), 0 10px 24px rgba(0,0,0,0.5);";

/// `hex` (`#rrggbb`) darkened toward the grid's background — `amount` of
/// the colour left — for a switch that is not lit. A plain colour, so the
/// dark state never depends on the renderer re-applying an opacity.
#[must_use]
pub fn dim(hex: &str, amount: f32) -> String {
    let h = hex.trim_start_matches('#');
    let ch =
        |i: usize| f32::from(u8::from_str_radix(h.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0));
    let base = [10.0, 10.0, 12.0];
    let mix = |c: f32, b: f32| (b + (c - b) * amount).round().clamp(0.0, 255.0) as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        mix(ch(0), base[0]),
        mix(ch(2), base[1]),
        mix(ch(4), base[2])
    )
}

/// The physical switch number, pinned to a tile corner.
#[component]
pub fn SwitchNo(no: usize) -> Element {
    rsx! {
        span {
            style: "position: absolute; top: 5px; left: 9px; font-size: 10px; font-family: monospace; opacity: 0.4;",
            "{no}"
        }
    }
}

/// A footswitch-shaped button with the tap/hold split every switch shares:
/// press-and-release fires `on_tap`; holding for [`HOLD_MS`] fires `on_hold`
/// instead (release then does nothing). Pointer events, so it behaves the
/// same with a mouse or a finger on a stage tablet. No `on_hold` → every
/// press is a tap, however long. A right-click is never a press — it opens
/// the tile's menu.
#[component]
pub fn HoldButton(
    #[props(default)] class: String,
    style: String,
    on_tap: Callback<()>,
    #[props(default)] on_hold: Option<Callback<()>>,
    /// Momentary: `on_down` on the press and `on_up` on the release (or on
    /// dragging off), in place of tap and hold.
    #[props(default)]
    on_down: Option<Callback<()>>,
    #[props(default)] on_up: Option<Callback<()>>,
    children: Element,
) -> Element {
    let mut hold_fired = use_signal(|| false);
    let mut hold_task = use_signal(|| None::<Task>);
    let mut held = use_signal(|| false);
    let secondary = |e: &PointerEvent| {
        matches!(
            e.trigger_button(),
            Some(dioxus::html::input_data::MouseButton::Secondary)
        )
    };
    rsx! {
        button {
            class: "{class}",
            style: "{style}",
            onpointerdown: move |e: PointerEvent| {
                if secondary(&e) {
                    return;
                }
                if let Some(down) = on_down {
                    held.set(true);
                    down.call(());
                    return;
                }
                hold_fired.set(false);
                if let Some(hold) = on_hold {
                    let task = spawn(async move {
                        architect::platform::sleep(Duration::from_millis(HOLD_MS)).await;
                        hold_fired.set(true);
                        hold.call(());
                    });
                    hold_task.set(Some(task));
                }
            },
            onpointerup: move |e: PointerEvent| {
                if secondary(&e) {
                    return;
                }
                if on_down.is_some() {
                    if held() {
                        held.set(false);
                        if let Some(up) = on_up {
                            up.call(());
                        }
                    }
                    return;
                }
                if let Some(task) = hold_task.take() {
                    task.cancel();
                }
                if !hold_fired() {
                    on_tap.call(());
                }
            },
            // The system took the touch back (a gesture, an alert): nothing
            // fires, and a held momentary lets go.
            onpointercancel: move |_| {
                if held() {
                    held.set(false);
                    if let Some(up) = on_up {
                        up.call(());
                    }
                }
                if let Some(task) = hold_task.take() {
                    task.cancel();
                }
            },
            onpointerleave: move |_| {
                // A held momentary lets go when the pointer leaves.
                if held() {
                    held.set(false);
                    if let Some(up) = on_up {
                        up.call(());
                    }
                }
                // Dragging off the switch cancels the press entirely.
                if let Some(task) = hold_task.take() {
                    task.cancel();
                }
            },
            {children}
        }
    }
}

/// How much of the page a rig's footswitches take.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SwitchesMode {
    /// The full grid, big tiles.
    #[default]
    Full,
    /// A short strip.
    Compact,
    /// No switches: the page gets all the height.
    Hidden,
}

impl SwitchesMode {
    /// The next in the header toggle's cycle.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Full => Self::Compact,
            Self::Compact => Self::Hidden,
            Self::Hidden => Self::Full,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Full => "Full",
            Self::Compact => "Compact",
            Self::Hidden => "Hidden",
        }
    }
}

/// The rig header's "Switches" button: click to cycle Full → Compact →
/// Hidden. `children` is its icon.
#[component]
pub fn SwitchesToggle(mode: Signal<SwitchesMode>, children: Element) -> Element {
    let now = mode();
    rsx! {
        button {
            class: if now == SwitchesMode::Hidden {
                "flex items-center h-7 px-2 rounded-md border border-border text-muted-foreground hover:text-foreground text-xs"
            } else {
                "flex items-center h-7 px-2 rounded-md bg-accent text-accent-foreground text-xs"
            },
            style: "display: flex; align-items: center; gap: 5px;",
            title: "Switches: {now.label()} — click for {now.next().label()}",
            onclick: move |_| mode.set(now.next()),
            {children}
            "Switches"
        }
    }
}

/// What a switch answers to, pinned to its top-right corner: the pedal
/// learned onto it, or — while it waits for one — a call to press it.
#[component]
pub fn LearnBadge(binding: Option<String>, learning: bool) -> Element {
    let base = "position: absolute; top: 5px; right: 7px; max-width: 70%; overflow: hidden; \
                text-overflow: ellipsis; white-space: nowrap; font-size: 8px; font-weight: 800; \
                letter-spacing: 0.08em; border-radius: 4px; padding: 1px 4px;";
    rsx! {
        if learning {
            span { style: "{base} background: #f59e0b; color: #1c1300;", "PRESS A PEDAL…" }
        } else if let Some(b) = binding {
            span { style: "{base} background: #00000066; color: #e4e4e7; opacity: 0.8;", "{b}" }
        }
    }
}

/// A row's style in a switch menu.
pub const MENU_ROW: &str = "display: flex; align-items: center; gap: 8px; padding: 6px 9px; \
                            border-radius: 6px; font-size: 11px; color: #d4d4d8; cursor: pointer; \
                            white-space: nowrap;";
/// A section heading's style in a switch menu.
pub const MENU_HEAD: &str = "padding: 6px 9px 3px; font-size: 9px; letter-spacing: 0.12em; \
                             text-transform: uppercase; color: #71717a;";

/// A switch menu's panel, headed `title`. It is placed by whoever shows
/// it: [`open_menu`] (the app's popup layer, above everything) or
/// [`InlineMenu`].
#[component]
pub fn SwitchMenuFrame(title: String, children: Element) -> Element {
    rsx! {
        div {
            style: "{MENU_PANEL}",
            // A press inside the menu is the menu's, not the tile's.
            onpointerdown: move |e: PointerEvent| e.stop_propagation(),
            div { style: "{MENU_HEAD}", "{title}" }
            {children}
        }
    }
}

/// A switch menu's panel style (see [`SwitchMenuFrame`]).
pub const MENU_PANEL: &str = "min-width: 220px; padding: 4px; display: flex; flex-direction: column; \
                              gap: 1px; border: 1px solid #2b2b31; border-radius: 10px; \
                              background: #0d0d10; box-shadow: 0 12px 32px #000c;";

/// Open a switch's menu **above** the pointer, in the app's popup layer —
/// over every panel, never clipped by the dock the switches sit in. The
/// switches are at the bottom of the window, so a menu that dropped down
/// would have nowhere to go.
pub fn open_menu(host: PopupHost, e: &MouseEvent, render: impl Fn() -> Element + 'static) {
    let p = e.client_coordinates();
    open_menu_at(host, p.x, p.y, render);
}

/// [`open_menu`] at client point `(x, y)` — for a long-press, which has a
/// point but no mouse event.
pub fn open_menu_at(host: PopupHost, x: f64, y: f64, render: impl Fn() -> Element + 'static) {
    host.open_up(x - 110.0, y - 6.0, 230.0, render, || {});
}

/// Close the open switch menu. After the click that picked from it is done
/// (see [`PopupHost`]'s layer: removing the node under a press crashed it).
pub fn close_menu(host: Option<PopupHost>) {
    if let Some(h) = host {
        spawn(async move { h.close() });
    }
}

/// A menu drawn in place, over its tile, growing upward from the tile's
/// bottom — the fallback where the app provides no popup layer.
#[component]
pub fn InlineMenu(children: Element) -> Element {
    rsx! {
        div {
            style: "position: absolute; bottom: 8px; right: 8px; z-index: 300; max-height: 70vh; overflow-y: auto;",
            {children}
        }
    }
}

/// One menu row: a mark (✓, an icon or nothing) and a label.
#[component]
pub fn MenuRow(
    label: String,
    #[props(default)] mark: String,
    #[props(default)] mark_color: Option<String>,
    #[props(default)] danger: bool,
    onclick: EventHandler<()>,
) -> Element {
    let color = if danger { "color: #fca5a5;" } else { "" };
    let mark_color = mark_color.unwrap_or_else(|| "#22c55e".into());
    rsx! {
        div {
            class: "hover:bg-accent/40",
            style: "{MENU_ROW} {color}",
            onclick: move |e: MouseEvent| {
                e.stop_propagation();
                onclick.call(());
            },
            span { style: "width: 12px; text-align: center; color: {mark_color};", "{mark}" }
            "{label}"
        }
    }
}

/// The MIDI learn rows every switch menu ends with: learn a pedal onto
/// `target`, stop waiting for one, or unbind it.
#[component]
pub fn MidiLearnRows(
    target: String,
    /// What it answers to now.
    binding: Option<String>,
    /// It is waiting for a pedal.
    learning: bool,
    on_learn: EventHandler<String>,
    on_cancel: EventHandler<()>,
    on_unlearn: EventHandler<String>,
    on_close: EventHandler<()>,
) -> Element {
    let (t1, t2) = (target.clone(), target);
    rsx! {
        div { style: "{MENU_HEAD}", "MIDI" }
        if learning {
            MenuRow {
                label: "Waiting — press a pedal or pad (click to stop)",
                mark: "●",
                mark_color: "#f59e0b".to_string(),
                onclick: move |()| {
                    on_cancel.call(());
                    on_close.call(());
                },
            }
        } else {
            MenuRow {
                label: match &binding {
                    Some(b) => format!("MIDI learn… (now {b})"),
                    None => "MIDI learn…".to_string(),
                },
                mark: "◎",
                mark_color: "#38bdf8".to_string(),
                onclick: move |()| {
                    on_learn.call(t1.clone());
                    on_close.call(());
                },
            }
        }
        if binding.is_some() {
            MenuRow {
                label: "Clear MIDI",
                danger: true,
                onclick: move |()| {
                    on_unlearn.call(t2.clone());
                    on_close.call(());
                },
            }
        }
    }
}

/// A plain switch tile — name, a line under it, a footer, lit in its
/// colours when active — with its learn badge and a right-click menu. For switches that recall a scene; a
/// rig with richer tiles builds its own from the pieces above.
#[component]
pub fn SwitchTile(
    /// The physical switch number (1-based).
    no: usize,
    label: String,
    #[props(default)] detail: String,
    #[props(default)] footer: String,
    /// Lit `(background, text)` colours; dark is derived.
    colors: (String, String),
    active: bool,
    #[props(default)] compact: bool,
    #[props(default)] binding: Option<String>,
    #[props(default)] learning: bool,
    on_tap: Callback<()>,
    #[props(default)] on_hold: Option<Callback<()>>,
    /// Right-click — or its ⋯ by touch: open the tile's menu (see
    /// [`open_menu`]).
    on_menu: EventHandler<MouseEvent>,
) -> Element {
    let (bg, fg) = colors;
    let (bg, fg, ring) = if active {
        (bg, fg, LIT_RING)
    } else {
        (dim(&bg, 0.3), dim(&fg, 0.45), "")
    };
    let learn_ring = if learning {
        "box-shadow: 0 0 0 2px #f59e0b;"
    } else {
        ""
    };
    let layout = if compact {
        "flex-direction: row; gap: 8px; border-radius: 8px;"
    } else {
        "flex-direction: column; gap: 3px; border-radius: 12px;"
    };
    rsx! {
        div {
            style: "position: relative; flex: 1; min-width: 0; height: 100%; display: flex; flex-direction: column;",
            oncontextmenu: move |e: MouseEvent| {
                e.prevent_default();
                on_menu.call(e);
            },
            HoldButton {
                style: format!(
                    "position: relative; height: 100%; width: 100%; display: flex; align-items: center; \
                     justify-content: center; appearance: none; border: none; padding: 16px 10px 10px; \
                     overflow: hidden; background-color: {bg}; color: {fg}; {layout} {ring} {learn_ring}"
                ),
                on_tap,
                on_hold,
                SwitchNo { no }
                LearnBadge { binding: binding.clone(), learning }
                span {
                    style: if compact { "font-size: 13px; font-weight: 700; letter-spacing: 0.04em;" }
                        else { "font-size: 18px; font-weight: 700; letter-spacing: 0.04em;" },
                    "{label}"
                }
                if !detail.is_empty() && !compact {
                    span { style: "font-size: 10px; opacity: 0.8; line-height: 1.25; text-align: center;", "{detail}" }
                }
                if !footer.is_empty() {
                    span { style: "font-size: 9px; opacity: 0.65; font-variant-numeric: tabular-nums;", "{footer}" }
                }
            }
            // A long-press is the tile's hold, so a finger opens the menu here.
            crate::touch::TouchMenuButton { onclick: move |e: MouseEvent| on_menu.call(e), title: "Switch menu" }
        }
    }
}
