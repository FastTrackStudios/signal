//! The iPad rig frame — one for every rig, so guitar, bass and drums read
//! as one instrument:
//!
//!   top bar    over the sidebar its toggle and its tabs ([`SideTab`]);
//!              then the rig's own (the kit, the player), the status
//!              lights ([`Indicator`]), the load ([`Cpu`]) and the meters
//!              ([`Meters`], [`MiniMeter`])
//!   sidebar    [`Sidebar`]: what the tabs over it pick
//!   main       the rig's own, and docked at its foot [`Dock`]
//!   foot bar   [`FootBar`] of [`FootButton`]s: the views as tabs, the
//!              docks as toggles ([`Group`] for one-at-a-time), Setup
//!              ([`Gear`]) at the right
//!
//! Setup holds the way back to the app's instruments ([`InstrumentsBack`],
//! when the app provides a [`PhoneHost`]) — two steps from the stage, so a
//! rig is never left by accident mid-song.
//!
//! Inline styles only: Blitz takes no stylesheet (CLAUDE.md).

pub mod tokens;

use dioxus::prelude::*;

use self::tokens::*;

/// The app around a rig on a phone or tablet: the way back to the
/// instrument menu (the app's front door). Absent where the rig is the
/// whole app (the Android remote, the desktop shot tool).
#[derive(Clone, Copy)]
pub struct PhoneHost {
    pub on_home: Callback<()>,
}

/// The window's size in points — what menus and sheets keep inside.
/// Provided by the host.
#[derive(Clone, Copy)]
pub struct WindowSize(pub Signal<(f64, f64)>);

// ── Frame ──────────────────────────────────────────────────────────────────

/// The whole surface: a column — top bar, the middle, foot bar — on the
/// desk, every button answering a press.
#[component]
pub fn Frame(
    #[props(default)] onpointerdown: Option<EventHandler<PointerEvent>>,
    children: Element,
) -> Element {
    rsx! {
        div {
            style: "position: relative; width: 100%; height: 100%; display: flex; flex-direction: column; background: {DESK}; color: {INK}; font-family: {FONT}; font-weight: 500; overflow: hidden;",
            onpointerdown: move |e| {
                if let Some(h) = onpointerdown {
                    h.call(e);
                }
            },
            document::Style { {PRESSABLE} }
            {children}
        }
    }
}

/// The middle: the sidebar and the main area side by side.
#[component]
pub fn Middle(children: Element) -> Element {
    rsx! {
        div { style: "flex: 1; min-height: 0; display: flex;", {children} }
    }
}

/// The main area, right of the sidebar: a column, its dock at the foot.
#[component]
pub fn Main(children: Element) -> Element {
    rsx! {
        main { style: "flex: 1; min-width: 0; height: 100%; display: flex; flex-direction: column; background: {DESK};", {children} }
    }
}

/// The sidebar, down the left.
#[component]
pub fn Sidebar(children: Element) -> Element {
    rsx! {
        aside { style: "width: {SIDEBAR_W}px; flex-shrink: 0; height: 100%; border-right: 1px solid {RULE}; box-sizing: border-box; display: flex; flex-direction: column; min-height: 0;",
            {children}
        }
    }
}

/// What is docked at the main area's foot (the switches, the FX row, a
/// drum piece).
#[component]
pub fn Dock(children: Element) -> Element {
    rsx! {
        div { style: "flex-shrink: 0;", {children} }
    }
}

// ── Top bar ────────────────────────────────────────────────────────────────

/// The top bar: over the sidebar its toggle and `side` (its tabs); then
/// `children`, the rig's own and its status, left to right.
#[component]
pub fn TopBar(
    sidebar: bool,
    on_sidebar: EventHandler<()>,
    side: Element,
    children: Element,
) -> Element {
    let ink = if sidebar { INK } else { INK_3 };
    rsx! {
        header { style: "height: {TOP_H}px; flex-shrink: 0; display: flex; align-items: stretch; border-bottom: 1px solid {RULE}; background: {SHEET}; box-sizing: border-box; font-family: {FONT};",
            div { style: "width: {SIDEBAR_W}px; flex-shrink: 0; display: flex; align-items: stretch; border-right: 1px solid {RULE}; box-sizing: border-box;",
                button {
                    "aria-label": if sidebar { "Hide the sidebar" } else { "Show the sidebar" },
                    style: "width: 52px; display: flex; align-items: center; justify-content: center; border: none; background: transparent; cursor: pointer;",
                    onclick: move |_| on_sidebar.call(()),
                    svg { key: "{sidebar}", width: "20", height: "20", view_box: "0 0 20 20",
                        rect { x: "2.5", y: "3.5", width: "15", height: "13", rx: "2.5", fill: "none", stroke: ink, stroke_width: "1.5" }
                        path { d: "M8 4v12", stroke: ink, stroke_width: "1.5" }
                    }
                }
                Rule {}
                div { style: "flex: 1; min-width: 0; display: flex; align-items: stretch;", {side} }
            }
            {children}
        }
    }
}

/// One of the tabs over the sidebar: its mark (an 18-unit path) and its
/// name; the one in play with the live green along its foot.
#[component]
pub fn SideTab(
    label: String,
    #[props(default)] aria: Option<String>,
    d: &'static str,
    on: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let tone = pick(on, INK, INK_3);
    let aria = aria.unwrap_or_else(|| label.clone());
    rsx! {
        button {
            "aria-label": "{aria}",
            "aria-pressed": "{on}",
            style: "position: relative; flex: 1; min-width: 0; display: flex; align-items: center; justify-content: center; border: none; background: transparent; font-size: 15px; font-weight: {pick(on, 750, 600)}; color: {tone}; font-family: {FONT}; cursor: pointer;",
            onclick: move |e| onclick.call(e),
            span { style: "display: flex; align-items: center; gap: 7px;",
                svg { key: "{tone}", width: "18", height: "18", view_box: "0 0 18 18", style: "flex-shrink: 0; display: block;",
                    path { d: "{d}", fill: "none", stroke: "{tone}", stroke_width: "1.6", stroke_linecap: "round", stroke_linejoin: "round" }
                }
                "{label}"
            }
            if on {
                span { style: "position: absolute; left: 10px; right: 10px; bottom: 0; height: 2px; border-radius: 1px; background: {LIVE};" }
            }
        }
    }
}

/// The performance modes every rig has, in order: what the sidebar
/// shows and what plays from it.
pub const MODES: [&str; 3] = ["Preset", "Profile", "Setlist"];

/// The mode tabs over the sidebar — Preset, Profile, Setlist — the one in
/// play with the live green under it. `mode` past 2 reads as Setlist.
/// `aria` names what a tab does with its mode's name (`"Footswitches play
/// the %"`); without it, the name.
#[component]
pub fn ModeTabs(
    mode: u32,
    on_pick: EventHandler<u32>,
    #[props(default)] aria: Option<&'static str>,
) -> Element {
    let on = mode.min(2);
    rsx! {
        for (m, label) in (0u32..).zip(MODES) {
            SideTab {
                key: "{m}",
                label: label.to_string(),
                aria: aria.map(|a| a.replace('%', label)),
                d: mode_mark(m),
                on: m == on,
                onclick: move |_| {
                    if m != on {
                        on_pick.call(m);
                    }
                },
            }
        }
    }
}

/// A mode's mark: a preset's layers, a profile's stacks, a setlist's lines.
#[must_use]
pub fn mode_mark(mode: u32) -> &'static str {
    match mode {
        0 => marks::LAYERS,
        1 => marks::GRID,
        _ => marks::LIST,
    }
}

/// The marks the side tabs use, by what they list.
pub mod marks {
    /// Layers: presets.
    pub const LAYERS: &str = "M9 2.5 15.5 6 9 9.5 2.5 6ZM2.5 9 9 12.5 15.5 9M2.5 12 9 15.5 15.5 12";
    /// Four squares: a profile's stacks, a library of kits.
    pub const GRID: &str =
        "M3 3.5h4.5v4.5H3ZM10.5 3.5H15v4.5h-4.5ZM3 10h4.5v4.5H3ZM10.5 10H15v4.5h-4.5Z";
    /// Lines: a setlist, a list of pieces.
    pub const LIST: &str = "M6 4.5h9M6 9h9M6 13.5h9M3 4.5h.01M3 9h.01M3 13.5h.01";
}

/// A hairline between top-bar and foot-bar groups.
#[component]
pub fn Rule() -> Element {
    rsx! { span { style: "width: 1px; align-self: center; height: 22px; background: {RULE}; flex-shrink: 0;" } }
}

/// The MIDI or Audio light: its icon coloured by how the link is (green
/// up, amber struggling, red down, grey not there); a press opens Setup on
/// its tab.
#[component]
pub fn Indicator(
    kind: &'static str,
    tone: &'static str,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let label = if kind == "midi" { "MIDI" } else { "Audio" };
    let says = if tone == VOID || tone == INK_3 {
        "down"
    } else {
        "connected"
    };
    rsx! {
        button {
            "aria-label": "{label} {says} — open {label} in Setup",
            style: "align-self: stretch; min-width: 44px; flex-shrink: 0; display: flex; align-items: center; justify-content: center; border: none; background: transparent; cursor: pointer;",
            onclick: move |e| onclick.call(e),
            svg { key: "{tone}", width: "17", height: "17", view_box: "0 0 16 16",
                if kind == "midi" {
                    circle { cx: "8", cy: "8", r: "6.2", fill: "none", stroke: tone, stroke_width: "1.4" }
                    for (x, y) in [(4.6, 8.0), (5.6, 5.4), (8.0, 4.4), (10.4, 5.4), (11.4, 8.0)] {
                        circle { key: "{x}", cx: "{x}", cy: "{y}", r: "0.9", fill: tone }
                    }
                } else {
                    path { d: "M1.5 8h2l1.5-4 2.5 8 2-6 1.5 4 1-2h2.5", fill: "none", stroke: tone, stroke_width: "1.4", stroke_linecap: "round", stroke_linejoin: "round" }
                }
            }
        }
    }
}

/// CPU: a chip, its load under it — quiet while easy, amber from 60%, red
/// from 85% (where xruns start).
#[component]
pub fn Cpu(cpu: f64) -> Element {
    let pct = (cpu * 100.0).round().clamp(0.0, 100.0) as i32;
    let tone = if pct >= 85 {
        VOID
    } else if pct >= 60 {
        MODIFIED
    } else {
        INK_3
    };
    rsx! {
        span { title: "DSP load on the rig: {pct}%", style: "display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 1px; padding: 0 6px; font-size: 12px; font-weight: 750; line-height: 1; color: {tone};",
            svg { key: "{tone}", width: "14", height: "14", view_box: "0 0 16 16",
                rect { x: "3.5", y: "3.5", width: "9", height: "9", rx: "1.5", fill: "none", stroke: tone, stroke_width: "1.4" }
                rect { x: "6", y: "6", width: "4", height: "4", rx: "0.5", fill: tone }
                path { d: "M6 1.5v2M10 1.5v2M6 12.5v2M10 12.5v2M1.5 6h2M1.5 10h2M12.5 6h2M12.5 10h2", stroke: tone, stroke_width: "1.2", stroke_linecap: "round" }
            }
            span { style: "font-variant-numeric: tabular-nums;", "{pct}%" }
        }
    }
}

/// A count in the place of the CPU chip — the voices a drum kit is
/// sounding.
#[component]
pub fn Count(n: u32, label: &'static str) -> Element {
    rsx! {
        span { title: "{label}: {n}", style: "display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 2px; padding: 0 8px; min-width: 32px; font-weight: 750; line-height: 1; color: {INK_3};",
            span { style: "font-size: 15px; color: {pick(n > 0, INK_2, INK_3)}; font-variant-numeric: tabular-nums;", "{n}" }
            span { style: "font-size: 10px; letter-spacing: 0.06em; text-transform: uppercase;", "{label}" }
        }
    }
}

/// IN, OUT and PHONES as three slim upright bars; a muted output goes red
/// and still.
#[component]
pub fn Meters(input: f64, output: f64, phones_level: f64, house: bool, phones: bool) -> Element {
    rsx! {
        MeterWell {
            MiniMeter { label: "I", level: input, muted: false, phones: false }
            MiniMeter { label: "O", level: output, muted: house, phones: false }
            MiniMeter { label: "", level: phones_level, muted: phones, phones: true }
        }
    }
}

/// The top bar's place for meters.
#[component]
pub fn MeterWell(children: Element) -> Element {
    rsx! {
        div { title: "IN · OUT · PHONES", style: "align-self: stretch; flex-shrink: 0; display: flex; gap: 5px; padding: 6px 8px 4px; box-sizing: border-box;", {children} }
    }
}

/// One slim upright meter, its label (or the phones) under it.
#[component]
pub fn MiniMeter(label: &'static str, level: f64, muted: bool, phones: bool) -> Element {
    let l = level.clamp(0.0, 1.0);
    // The gradient is the bar's full height, shown up to the level.
    let inner = 100.0 / l.max(0.01);
    let ground = if muted {
        "color-mix(in oklab, #f87171 55%, #000)"
    } else {
        WELL
    };
    let ink = if muted { VOID } else { INK_3 };
    rsx! {
        div { style: "display: flex; flex-direction: column; align-items: center; gap: 2px; min-height: 0; min-width: 10px;",
            span { style: "position: relative; flex: 1; width: 6px; min-height: 0; border-radius: 3px; overflow: hidden; background: {ground};",
                if !muted {
                    span { style: "position: absolute; left: 0; right: 0; bottom: 0; height: {pct(l)}%; overflow: hidden;",
                        span { style: "position: absolute; left: 0; right: 0; bottom: 0; height: {inner}%; background: linear-gradient(0deg, #15803d 0%, #22c55e 60%, #eab308 82%, #f87171 100%);" }
                    }
                }
            }
            span { style: "height: 12px; display: flex; align-items: center; font-size: 12px; font-weight: 800; line-height: 1; color: {ink};",
                if phones {
                    svg { key: "{muted}", width: "12", height: "11", view_box: "0 0 16 14",
                        path { d: "M2.5 9V7.5a5.5 5.5 0 0 1 11 0V9", fill: "none", stroke: ink, stroke_width: "2" }
                        rect { x: "1.2", y: "8.2", width: "3.6", height: "5", rx: "1.2", fill: ink }
                        rect { x: "11.2", y: "8.2", width: "3.6", height: "5", rx: "1.2", fill: ink }
                    }
                } else {
                    "{label}"
                }
            }
        }
    }
}

// ── Foot bar ───────────────────────────────────────────────────────────────

/// The foot bar: the views, the docks, Setup.
#[component]
pub fn FootBar(children: Element) -> Element {
    rsx! {
        footer { style: "height: {FOOT_H}px; flex-shrink: 0; display: flex; align-items: stretch; border-top: 1px solid {RULE}; background: {SHEET}; padding: 0 6px; box-sizing: border-box;",
            {children}
        }
    }
}

/// An icon's ink, by its button's state — explicit, since Blitz does not
/// re-resolve `currentColor` inside an SVG on restyle.
#[must_use]
pub fn ink(on: bool) -> &'static str {
    if on { INK } else { INK_3 }
}

/// A foot-bar button. A view (Perform, Build, Edit, Setup) is a tab: the
/// one open is bright with a bar along its top edge. A dock (`pin`) is a
/// toggle: on, it sits in a fill.
#[component]
pub fn FootButton(
    label: &'static str,
    on: bool,
    pin: bool,
    #[props(default)] grouped: bool,
    onclick: EventHandler<MouseEvent>,
    children: Element,
) -> Element {
    let colour = ink(on);
    let bg = if pin && on { FILL_ON } else { CLEAR };
    let margin = if grouped {
        "0"
    } else {
        pick(pin, PIN_MARGIN, TAB_MARGIN)
    };
    rsx! {
        button {
            title: "{label}",
            "aria-pressed": "{on}",
            style: "position: relative; min-width: 64px; margin: {margin}; padding: 0 10px; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 4px; border: none; border-radius: {pick(pin, 8, 0)}px; background: {bg}; color: {colour}; font-family: {FONT}; cursor: pointer;",
            onclick: move |e| onclick.call(e),
            if !pin && on {
                span { style: "position: absolute; top: 0; left: 12px; right: 12px; height: 3px; border-radius: 0 0 3px 3px; background: {INK};" }
            }
            // Keyed by its state: rebuilt, not restyled, when it changes.
            svg { key: "{on}", width: "20", height: "20", view_box: "0 0 18 18", style: "display: block;", {children} }
            span { style: "font-size: 12px; font-weight: {pick(on, 700, 600)}; letter-spacing: 0.01em; color: {colour};", "{label}" }
        }
    }
}

/// Setup's foot button, at the right edge.
#[component]
pub fn SetupButton(on: bool, onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        FootButton { label: "Setup", on, pin: false, onclick: move |e| onclick.call(e),
            Gear { colour: ink(on) }
        }
    }
}

/// Toggles that share one place, side by side in a well: one of them on
/// at a time.
#[component]
pub fn Group(children: Element) -> Element {
    rsx! {
        div { style: "display: flex; align-items: stretch; gap: 2px; margin: 5px 4px; padding: 2px; border-radius: 10px; background: rgba(255,255,255,0.04);", {children} }
    }
}

/// A gear: teeth around a hub.
#[component]
pub fn Gear(colour: &'static str) -> Element {
    let teeth = 8;
    let points: String = (0..teeth * 4)
        .map(|i| {
            let a = (f64::from(i) / f64::from(teeth * 4)) * std::f64::consts::TAU
                - std::f64::consts::FRAC_PI_2;
            let r = if i % 4 == 1 || i % 4 == 2 { 7.6 } else { 5.9 };
            format!("{:.2},{:.2}", 9.0 + r * a.cos(), 9.0 + r * a.sin())
        })
        .collect::<Vec<_>>()
        .join(" ");
    rsx! {
        polygon { points: "{points}", fill: "none", stroke: colour, stroke_width: "1.4", stroke_linejoin: "round" }
        circle { cx: "9", cy: "9", r: "2.4", fill: "none", stroke: colour, stroke_width: "1.4" }
    }
}

// ── Setup ──────────────────────────────────────────────────────────────────

/// Setup's first cell: back to the app's instruments, when the app has a
/// menu to go back to.
#[component]
pub fn InstrumentsBack() -> Element {
    let Some(host) = try_use_context::<PhoneHost>() else {
        return rsx! {};
    };
    rsx! {
        button {
            "aria-label": "Back to the instruments",
            style: "height: 100%; min-width: {HIT}px; display: flex; align-items: center; gap: 8px; padding: 0 18px; border: none; border-right: 1px solid {RULE}; background: transparent; color: {INK_2}; font-size: 15px; font-weight: 650; font-family: {FONT}; cursor: pointer;",
            onclick: move |_| host.on_home.call(()),
            svg { width: "9", height: "15", view_box: "0 0 9 15",
                path { d: "M7.5 1.5 1.5 7.5l6 6", fill: "none", stroke: INK_2, stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round" }
            }
            "Instruments"
        }
    }
}

/// Whether the app provides the way back (Setup's header makes room).
#[must_use]
pub fn has_host() -> bool {
    try_use_context::<PhoneHost>().is_some()
}

/// A pressed button sinks a little and lightens — over its own background,
/// which is inline, hence `!important`.
pub const PRESSABLE: &str = "button:not(:disabled):active { transform: scale(0.985); background-image: linear-gradient(rgba(255,255,255,0.07), rgba(255,255,255,0.07)) !important; }";
