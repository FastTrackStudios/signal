//! **The kit** — the pieces every selector and manager in the rig is built
//! from, so choosing and managing look and behave the same on every surface:
//! a module's presets on the control surface, the right sidebar, the preset,
//! setlist and profile sidebars, and the library.
//!
//! - [`PresetBar`]: what plays (name · snapshot), a modified dot when the live
//!   sound differs from what is saved, ‹ › to step, ▾ for the list (with a
//!   search once it is long) and ⋯ for the management actions.
//! - [`ListRow`]: one row of any list — a status dot, title and sub-line, and
//!   the same ⋯ menu on the row (right-click opens it too, as on the perform
//!   grid's switches).
//! - [`ActionMenu`] / [`MenuItem`]: the menu. An item runs, asks for a name in
//!   place (Rename…, Save as…, Duplicate…) or deletes in two clicks — and a
//!   delete the rig would refuse is shown disabled, with the reason.
//! - [`SectionHeader`], [`Button`], [`IconButton`], [`DeleteButton`],
//!   [`NamePrompt`], [`Chips`], [`Dot`] for everything around them.
//!
//! Menus and lists open through the app root's [`PopupHost`], so a panel that
//! clips its overflow cannot cut them off; without a host (a surface mounted
//! on its own) they drop inline under their button instead.
//!
//! A menu item is data with an `id`, and the owner handles every item in one
//! `on_pick` — no callback per item, which would be made anew on every render
//! (see `stable`).

use dioxus::prelude::*;
use signal_widgets::PopupHost;
// The menus, prompts and small pieces live in `signal_widgets::kit`, shared
// with the keys rig; the rig's own names for them stay.
pub use signal_widgets::kit::{
    ActionMenu, Button, Chips, DeleteButton, Dot, MenuItem, NamePrompt, Picked, SectionHeader,
    context_menu, origin_of,
};

use crate::theme::{
    DANGER, DIM, FAINT, FIELD, FOCUS_BG, FOCUS_FG, LINE, LINE_STRONG, LIVE_BG, MENU, MUTED, R_MD,
    R_SM, T_BODY, T_META, T_SMALL, TEXT,
};

/// A square icon button.
#[component]
pub fn IconButton(
    icon: fts_chrome::Icon,
    title: String,
    #[props(default)] disabled: bool,
    #[props(default)] danger: bool,
    /// Lit, for a toggle that is on.
    #[props(default)]
    active: bool,
    /// Upside down — a ChevronDown as an up arrow.
    #[props(default)]
    flip: bool,
    #[props(default = 24)] size: u32,
    onclick: EventHandler<()>,
) -> Element {
    let colour = if disabled {
        DIM
    } else if danger {
        DANGER
    } else if active {
        FOCUS_FG
    } else {
        MUTED
    };
    let bg = if active { FOCUS_BG } else { "transparent" };
    let rotate = if flip {
        "transform: rotate(180deg);"
    } else {
        ""
    };
    let glyph = (size / 2).max(10);
    let cursor = if disabled { "default" } else { "pointer" };
    rsx! {
        button {
            style: "display: flex; align-items: center; justify-content: center; width: {size}px; \
                    height: {size}px; flex-shrink: 0; padding: 0; border-radius: {R_SM}; \
                    border: 1px solid {LINE}; background: {bg}; color: {colour}; \
                    cursor: {cursor};",
            title: "{title}",
            disabled,
            onclick: move |e: MouseEvent| {
                e.stop_propagation();
                if !disabled {
                    onclick.call(());
                }
            },
            span { style: "display: flex; {rotate}", fts_chrome::Glyph { icon, size: glyph } }
        }
    }
}

// ── Rows ───────────────────────────────────────────────────────────────────

/// One row of a list: a status dot, a title over a sub-line, an optional
/// right-hand note, and the row's menu (⋯, or a right-click).
#[component]
pub fn ListRow(
    title: String,
    #[props(default)] sub: String,
    /// A short note at the right (key · tempo, a count).
    #[props(default)]
    note: String,
    #[props(default)] live: bool,
    #[props(default)] selected: bool,
    #[props(default)] modified: bool,
    /// Indent, px — a snapshot under its preset.
    #[props(default)]
    indent: u32,
    /// Smaller type, for rows nested under another.
    #[props(default)]
    small: bool,
    /// A module glyph at the head (`Amp`, `Delay`) — what kind of thing the
    /// row is, before its name is read.
    #[props(default)]
    glyph: String,
    /// A colour swatch at the head — a stack's folder colour.
    #[props(default)]
    swatch: String,
    onclick: EventHandler<()>,
    #[props(default)] ondoubleclick: Option<EventHandler<()>>,
    #[props(default)] menu: Vec<MenuItem>,
    #[props(default)] on_menu: Option<EventHandler<Picked>>,
    /// Extra controls before the menu.
    #[props(default)]
    children: Element,
) -> Element {
    let (bg, fg) = if selected {
        (FOCUS_BG, FOCUS_FG)
    } else if live {
        (LIVE_BG, TEXT)
    } else {
        ("transparent", if small { MUTED } else { TEXT })
    };
    let (title_size, pad) = if small {
        ("12px", "5px 8px")
    } else {
        ("13px", "7px 8px")
    };
    let weight = if small { 500 } else { 600 };
    let host = PopupHost::try_use();
    let has_menu = !menu.is_empty() && on_menu.is_some();
    // The glyph takes the row's state colour: grey at rest.
    let glyph_colour = if live || selected { fg } else { FAINT };
    rsx! {
        div {
            class: if selected || live { "group" } else { "group hover:bg-accent/30" },
            style: "display: flex; align-items: center; gap: 8px; min-width: 0; margin-left: {indent}px; \
                    padding: {pad}; border-radius: {R_SM}; cursor: pointer; background: {bg}; color: {fg};",
            onclick: move |_| onclick.call(()),
            ondoubleclick: move |_| {
                if let Some(h) = ondoubleclick {
                    h.call(());
                }
            },
            oncontextmenu: {
                let menu = menu.clone();
                move |e: MouseEvent| {
                    if let Some(h) = on_menu {
                        e.prevent_default();
                        e.stop_propagation();
                        context_menu(host, &e, menu.clone(), h);
                    }
                }
            },
            Dot { live, modified, hollow: !small }
            if !swatch.is_empty() {
                span { style: "width: 8px; height: 8px; border-radius: 2px; flex-shrink: 0; background: {swatch};" }
            }
            if !glyph.is_empty() {
                span { style: "display: flex; flex-shrink: 0; color: {glyph_colour};",
                    crate::icons::ModuleGlyph { module: glyph.clone(), size: 13 }
                }
            }
            div { style: "flex: 1 1 0; min-width: 0; display: flex; flex-direction: column; gap: 1px;",
                span { style: "font-size: {title_size}; font-weight: {weight}; white-space: nowrap; overflow: hidden;",
                    "{title}"
                }
                if !sub.is_empty() {
                    span { style: "font-size: {T_META}; color: {FAINT}; white-space: nowrap; overflow: hidden;", "{sub}" }
                }
            }
            if !note.is_empty() {
                span { style: "flex-shrink: 0; font-size: {T_META}; font-family: monospace; color: {FAINT}; white-space: nowrap;",
                    "{note}"
                }
            }
            {children}
            if let (true, Some(h)) = (has_menu, on_menu) {
                div {
                    class: if selected || live { "" } else { "opacity-40 group-hover:opacity-100" },
                    style: "display: flex; flex-shrink: 0;",
                    ActionMenu { items: menu.clone(), on_pick: h, size: 20, bare: true, title: "Actions" }
                }
            }
        }
    }
}

// ── The preset bar ─────────────────────────────────────────────────────────

/// One choice in a preset bar's list.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct PickOption {
    pub label: String,
    /// A heading it sits under (a snapshot's preset); a new group starts a
    /// heading row.
    pub group: String,
    /// Small text at its right.
    pub note: String,
    pub live: bool,
}

/// How many options before the list grows a search field.
const SEARCH_AFTER: usize = 8;

/// The list a preset bar drops: its options, grouped, searchable when long.
#[component]
pub fn PickList(
    options: Vec<PickOption>,
    on_pick: EventHandler<usize>,
    on_close: EventHandler<()>,
) -> Element {
    let mut query = use_signal(String::new);
    let q = query();
    let words: Vec<String> = q.split_whitespace().map(str::to_lowercase).collect();
    let hits: Vec<(usize, PickOption)> = options
        .iter()
        .cloned()
        .enumerate()
        .filter(|(_, o)| {
            let hay = format!("{} {} {}", o.group, o.label, o.note).to_lowercase();
            words.iter().all(|w| hay.contains(w))
        })
        .collect();
    let first = hits.first().map(|(i, _)| *i);
    let searchable = options.len() > SEARCH_AFTER;
    rsx! {
        div {
            style: "display: flex; flex-direction: column; gap: 1px; padding: 4px; min-width: 240px; \
                    max-width: 360px; max-height: 420px; border: 1px solid {LINE_STRONG}; \
                    border-radius: {R_MD}; background: {MENU}; box-shadow: 0 12px 32px #000c;",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            if searchable {
                input {
                    style: "flex-shrink: 0; margin: 2px 2px 4px; font-size: {T_BODY}; color: {TEXT}; background: {FIELD}; \
                            border: 1px solid {LINE_STRONG}; border-radius: {R_SM}; padding: 6px 8px; outline: none;",
                    placeholder: "Search…",
                    value: "{q}",
                    onmounted: move |e| {
                        spawn(async move {
                            let _ = e.data().set_focus(true).await;
                        });
                    },
                    oninput: move |e| query.set(e.value()),
                    onkeydown: move |e: KeyboardEvent| {
                        e.stop_propagation();
                        match e.key() {
                            Key::Enter => {
                                if let Some(i) = first {
                                    on_pick.call(i);
                                    on_close.call(());
                                }
                            }
                            Key::Escape => on_close.call(()),
                            _ => {}
                        }
                    },
                }
            }
            div { style: "display: flex; flex-direction: column; gap: 1px; min-height: 0; overflow-y: scroll; scrollbar-width: thin;",
                if hits.is_empty() {
                    div { style: "padding: 8px 9px; font-size: {T_SMALL}; color: {FAINT};",
                        if options.is_empty() { "Nothing saved yet." } else { "No match." }
                    }
                }
                for (n, (i, o)) in hits.iter().cloned().enumerate() {
                    {
                        let new_group = !o.group.is_empty()
                            && (n == 0 || hits[n - 1].1.group != o.group);
                        rsx! {
                            div { key: "{i}", style: "display: contents;",
                            if new_group {
                                div {
                                    style: "padding: 6px 9px 2px; font-size: 9px; letter-spacing: 0.12em; \
                                            text-transform: uppercase; color: #71717a; white-space: nowrap; overflow: hidden;",
                                    "{o.group}"
                                }
                            }
                            div {
                                class: if o.live { "" } else { "hover:bg-accent/40" },
                                style: format!(
                                    "display: flex; align-items: center; gap: 8px; padding: 6px 9px; border-radius: {R_SM}; \
                                     font-size: {T_BODY}; cursor: pointer; white-space: nowrap; overflow: hidden; \
                                     background: {}; color: {}; {}",
                                    if o.live { LIVE_BG } else { "transparent" },
                                    if o.live { TEXT } else { "#d4d4d8" },
                                    if o.group.is_empty() { "" } else { "padding-left: 16px;" },
                                ),
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    on_pick.call(i);
                                    on_close.call(());
                                },
                                Dot { live: o.live, hollow: true }
                                span { style: "flex: 1 1 0; min-width: 0; overflow: hidden;", "{o.label}" }
                                if !o.note.is_empty() {
                                    span { style: "flex-shrink: 0; font-size: {T_META}; color: {FAINT};", "{o.note}" }
                                }
                            }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// **The preset bar**: what plays, whether it is edited, and every way to
/// change it — the one selector for module presets, block presets and the
/// sets and profiles that hold them.
///
/// `name` is what plays (empty: nothing picked yet), `sub` its snapshot. ▾
/// (or the name, unless `on_label` claims it) drops `options`; ‹ › call
/// `on_step`; ⋯ shows `menu`. `compact` is the control surface's size.
#[component]
pub fn PresetBar(
    /// The eyebrow: the module, the block, "Setlist".
    label: String,
    name: String,
    #[props(default)] sub: String,
    #[props(default)] modified: bool,
    #[props(default)] live: bool,
    options: Vec<PickOption>,
    on_pick: EventHandler<usize>,
    #[props(default)] on_step: Option<EventHandler<i32>>,
    #[props(default)] menu: Vec<MenuItem>,
    #[props(default)] on_menu: Option<EventHandler<Picked>>,
    /// A click on the name does this instead of opening the list.
    #[props(default)]
    on_label: Option<EventHandler<()>>,
    #[props(default)] compact: bool,
    /// A surface's heading: the name large, no box around the bar — the
    /// region's own edges hold it (the setlist sidebar's set).
    #[props(default)]
    large: bool,
    #[props(default)] placeholder: String,
    /// Extra inline style for the bar (width, flex).
    #[props(default)]
    style: String,
) -> Element {
    let host = PopupHost::try_use();
    let mut open = use_signal(|| false);
    // Compact fills its row (22–30px on the control surface); the list drops
    // below whichever it is.
    let (h, height): (u32, &str) = if compact {
        (30, "100%")
    } else if large {
        (52, "52px")
    } else {
        (54, "54px")
    };
    let side = if compact { 18 } else { 26 };
    let (eyebrow, title) = if compact {
        ("8px", "10px")
    } else if large {
        ("9px", "17px")
    } else {
        ("9px", "13px")
    };
    // The box, and the hairlines between its parts: none on a heading.
    let (frame, sep) = if large {
        (
            "border: none; background: transparent;".to_string(),
            "transparent",
        )
    } else {
        (
            format!("border: 1px solid {LINE_STRONG}; background: {FIELD};"),
            LINE,
        )
    };
    let (radius, name_pad, menu_pad) = if compact {
        ("4px", "5px", "0px")
    } else if large {
        ("8px", "0px", "0px")
    } else {
        ("8px", "9px", "3px")
    };
    let drop_bg = if open() { FOCUS_BG } else { "transparent" };
    let empty = if placeholder.is_empty() {
        "—".to_string()
    } else {
        placeholder.clone()
    };
    let toggle = {
        let options = options.clone();
        move |e: &MouseEvent| {
            if open() {
                open.set(false);
                if let Some(h) = host {
                    h.close();
                }
                return;
            }
            open.set(true);
            let Some(host) = host else { return };
            let (x, y) = origin_of(e);
            let options = options.clone();
            host.open(
                x,
                y + f64::from(h) + 2.0,
                240.0,
                move || {
                    rsx! {
                        PickList {
                            options: options.clone(),
                            on_pick,
                            on_close: move |()| {
                                spawn(async move { host.close() });
                            },
                        }
                    }
                },
                move || {
                    let mut o = open;
                    o.set(false);
                },
            );
        }
    };
    let step_btn = |delta: i32, glyph: &'static str, tip: &'static str| {
        rsx! {
            button {
                style: "display: flex; align-items: center; justify-content: center; width: {side}px; \
                        height: 100%; flex-shrink: 0; padding: 0; border: none; border-left: 1px solid {sep}; \
                        background: transparent; color: {MUTED}; cursor: pointer; font-size: {title};",
                title: "{tip}",
                onpointerdown: move |e: PointerEvent| e.stop_propagation(),
                onclick: move |e: MouseEvent| {
                    e.stop_propagation();
                    if let Some(s) = on_step {
                        s.call(delta);
                    }
                },
                "{glyph}"
            }
        }
    };
    rsx! {
        div {
            style: "position: relative; display: flex; align-items: stretch; height: {height}; min-width: 0; \
                    {frame} border-radius: {radius}; overflow: visible; {style}",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            // ▾ — the list. A heading's name opens it instead, so the name
            // keeps the left edge.
            if !large {
                button {
                    style: "display: flex; align-items: center; justify-content: center; width: {side}px; \
                            flex-shrink: 0; padding: 0; border: none; border-right: 1px solid {sep}; \
                            background: {drop_bg}; color: {MUTED}; cursor: pointer;",
                    title: "Choose {label}",
                    onpointerdown: move |e: PointerEvent| e.stop_propagation(),
                    onclick: {
                        let mut toggle = toggle.clone();
                        move |e: MouseEvent| {
                            e.stop_propagation();
                            toggle(&e);
                        }
                    },
                    fts_chrome::Glyph { icon: fts_chrome::Icon::ChevronDown, size: if compact { 10 } else { 12 } }
                }
            }
            // The name: what plays, and whether it is edited.
            div {
                class: "hover:bg-accent/30",
                style: "flex: 1 1 0; min-width: 0; display: flex; flex-direction: column; justify-content: center; \
                        gap: 1px; padding: 0 {name_pad}; cursor: pointer; line-height: 1.1;",
                title: if modified { format!("{label}: {name} — edited, not saved") } else { format!("{label}: {name}") },
                onpointerdown: move |e: PointerEvent| e.stop_propagation(),
                onclick: {
                    let mut toggle = toggle.clone();
                    move |e: MouseEvent| {
                        e.stop_propagation();
                        match on_label {
                            Some(cb) => cb.call(()),
                            None => toggle(&e),
                        }
                    }
                },
                span { style: "font-size: {eyebrow}; font-weight: 700; letter-spacing: 0.12em; text-transform: uppercase; \
                               color: {FAINT}; white-space: nowrap; overflow: hidden;",
                    "{label}"
                }
                div { style: "display: flex; align-items: center; gap: 5px; min-width: 0;",
                    if modified || live {
                        Dot { live, modified, size: if compact { 5 } else { 6 } }
                    }
                    span { style: "font-size: {title}; font-weight: 700; color: {TEXT}; white-space: nowrap; overflow: hidden; min-width: 0;",
                        if name.is_empty() { "{empty}" } else { "{name}" }
                        // Compact: one line, the snapshot after the name.
                        if compact && !sub.is_empty() {
                            span { style: "font-weight: 500; color: {MUTED};", " · {sub}" }
                        }
                    }
                }
                // Full size: the snapshot on a line of its own, so a long
                // name never pushes it out of the bar.
                if !compact && !sub.is_empty() {
                    span { style: "font-size: {T_SMALL}; color: {MUTED}; white-space: nowrap; overflow: hidden;", "{sub}" }
                }
            }
            if on_step.is_some() {
                {step_btn(-1, "‹", "Previous")}
                {step_btn(1, "›", "Next")}
            }
            if let Some(h) = on_menu.filter(|_| !menu.is_empty()) {
                div { style: "display: flex; align-items: center; justify-content: center; flex-shrink: 0; \
                              border-left: 1px solid {sep}; padding: 0 {menu_pad};",
                    ActionMenu { items: menu.clone(), on_pick: h, size: if compact { 18 } else { 26 }, bare: true, title: "{label} actions" }
                }
            }
            if open() && host.is_none() {
                div { style: "position: absolute; top: calc(100% + 2px); left: 0; z-index: 300;",
                    PickList { options: options.clone(), on_pick, on_close: move |()| open.set(false) }
                }
            }
        }
    }
}
