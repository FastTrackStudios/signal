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

use crate::theme::{
    DANGER, DANGER_INK, DANGER_LINE, DIM, EYEBROW, FAINT, FIELD, FOCUS_BG, FOCUS_FG, LINE,
    LINE_STRONG, LIVE, LIVE_BG, MENU, MODIFIED, MUTED, PRIMARY, R_MD, R_SM, T_BODY, T_META, T_SMALL, TEXT,
};

// ── Small pieces ───────────────────────────────────────────────────────────

/// The status dot at the head of a row or a bar: playing (green), modified
/// (amber — the live sound is not what is saved), else idle.
#[component]
pub fn Dot(
    #[props(default)] live: bool,
    #[props(default)] modified: bool,
    /// Draw nothing when idle rather than a grey dot.
    #[props(default)]
    hollow: bool,
    #[props(default = 6)] size: u32,
) -> Element {
    let (bg, title) = match (modified, live) {
        (true, _) => (MODIFIED, "Edited — not saved"),
        (false, true) => (LIVE, "Playing"),
        (false, false) if hollow => ("transparent", ""),
        (false, false) => (DIM, ""),
    };
    rsx! {
        span {
            style: "width: {size}px; height: {size}px; border-radius: 999px; flex-shrink: 0; background: {bg};",
            title: "{title}",
        }
    }
}

/// The label over a group, with an optional count and actions at its right.
#[component]
pub fn SectionHeader(
    label: String,
    #[props(default)] count: String,
    #[props(default)] children: Element,
) -> Element {
    rsx! {
        div { style: "display: flex; align-items: center; gap: 6px; min-height: 22px; min-width: 0;",
            span { style: "{EYEBROW}", "{label}" }
            if !count.is_empty() {
                span { style: "font-size: {T_META}; color: {FAINT};", "{count}" }
            }
            div { style: "flex: 1 1 0;" }
            {children}
        }
    }
}

/// A text button. `primary` is the one thing you came to do.
#[component]
pub fn Button(
    label: String,
    #[props(default)] primary: bool,
    #[props(default)] disabled: bool,
    /// Stretch to share a row with its neighbours.
    #[props(default)]
    grow: bool,
    /// Header size: for a "+ New" beside a section's label.
    #[props(default)]
    small: bool,
    #[props(default)] title: String,
    onclick: EventHandler<()>,
) -> Element {
    let (pad, font) = if small { ("3px 8px", T_SMALL) } else { ("6px 12px", T_BODY) };
    let (bg, fg, border) = match (primary, disabled) {
        (_, true) => ("transparent", FAINT, LINE),
        (true, false) => (PRIMARY, "#ffffff", PRIMARY),
        (false, false) => ("transparent", TEXT, "#34343c"),
    };
    let cursor = if disabled { "default" } else { "pointer" };
    let flex = if grow { "flex: 1 1 0; min-width: 0;" } else { "flex-shrink: 0;" };
    rsx! {
        button {
            style: "{flex} padding: {pad}; border-radius: {R_SM}; font-size: {font}; font-weight: 600; \
                    background: {bg}; color: {fg}; border: 1px solid {border}; cursor: {cursor}; \
                    white-space: nowrap; overflow: hidden;",
            title: "{title}",
            disabled,
            onclick: move |e: MouseEvent| {
                e.stop_propagation();
                if !disabled {
                    onclick.call(());
                }
            },
            "{label}"
        }
    }
}

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
    let rotate = if flip { "transform: rotate(180deg);" } else { "" };
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

/// Delete, in two steps — the first click arms it, the second deletes.
/// Disabled, with the reason as its tooltip and beneath it, when the rig
/// would refuse.
#[component]
pub fn DeleteButton(
    #[props(default)] refused: Option<String>,
    #[props(default = "Delete".to_string())] label: String,
    on_delete: EventHandler<()>,
) -> Element {
    let mut armed = use_signal(|| false);
    if let Some(why) = refused {
        return rsx! {
            Button { label, disabled: true, title: why, onclick: |()| {} }
        };
    }
    rsx! {
        button {
            style: format!(
                "flex-shrink: 0; padding: 6px 12px; border-radius: {R_SM}; font-size: {T_BODY}; font-weight: 600; \
                 cursor: pointer; background: {}; color: {}; border: 1px solid {};",
                if armed() { DANGER } else { "transparent" },
                if armed() { DANGER_INK } else { DANGER },
                if armed() { DANGER } else { DANGER_LINE },
            ),
            onclick: move |e: MouseEvent| {
                e.stop_propagation();
                if armed() {
                    on_delete.call(());
                    armed.set(false);
                } else {
                    armed.set(true);
                }
            },
            onmouseleave: move |_| armed.set(false),
            if armed() { "Click again to delete" } else { "{label}" }
        }
    }
}

/// Why `name` cannot be used: blank, or taken (case-insensitively).
#[must_use]
pub fn name_problem(name: &str, taken: &[String]) -> Option<String> {
    let n = name.trim();
    if n.is_empty() {
        Some("Give it a name".to_string())
    } else if taken.iter().any(|t| t.trim().eq_ignore_ascii_case(n)) {
        Some(format!("\"{n}\" is taken"))
    } else {
        None
    }
}

/// A name typed in place — Duplicate, Rename, Save as, New: a field, a
/// confirm and a cancel. Enter confirms, Escape cancels; a blank or taken
/// name says so and cannot be confirmed.
#[component]
pub fn NamePrompt(
    label: String,
    initial: String,
    #[props(default)] taken: Vec<String>,
    #[props(default)] placeholder: String,
    /// Accept an empty value (a path that may be cleared), not only a name.
    #[props(default)]
    allow_blank: bool,
    on_done: EventHandler<Option<String>>,
) -> Element {
    let mut text = use_signal(|| initial.clone());
    let problem = if allow_blank { None } else { name_problem(&text(), &taken) };
    let ok = problem.is_none();
    let commit = move || {
        if ok {
            let t = text.peek().trim().to_string();
            on_done.call(Some(t));
        }
    };
    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 4px; min-width: 0;",
            div { style: "display: flex; align-items: center; gap: 6px; min-width: 0;",
                input {
                    style: "flex: 1 1 0; min-width: 0; font-size: {T_BODY}; color: {TEXT}; background: {FIELD}; \
                            border: 1px solid {LINE_STRONG}; border-radius: {R_SM}; padding: 6px 8px; outline: none;",
                    value: "{text}",
                    placeholder: "{placeholder}",
                    onmounted: move |e| {
                        spawn(async move {
                            let _ = e.data().set_focus(true).await;
                        });
                    },
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    oninput: move |e| text.set(e.value()),
                    onkeydown: move |e: KeyboardEvent| {
                        e.stop_propagation();
                        match e.key() {
                            Key::Enter => commit(),
                            Key::Escape => on_done.call(None),
                            _ => {}
                        }
                    },
                }
                Button { label: label.clone(), primary: true, disabled: !ok, onclick: move |()| commit() }
                Button { label: "Cancel", onclick: move |()| on_done.call(None) }
            }
            if let Some(p) = problem.filter(|_| text() != initial) {
                span { style: "font-size: {T_META}; color: {FAINT};", "{p}" }
            }
        }
    }
}

/// A choice among a few names, as chips.
#[component]
pub fn Chips(
    options: Vec<String>,
    #[props(default)] selected: String,
    on_pick: EventHandler<String>,
) -> Element {
    rsx! {
        div { style: "display: flex; flex-wrap: wrap; gap: 5px;",
            for opt in options {
                {
                    let on = opt.eq_ignore_ascii_case(&selected);
                    let pick = opt.clone();
                    rsx! {
                        button {
                            key: "{opt}",
                            style: format!(
                                "padding: 4px 10px; border-radius: 999px; font-size: {T_SMALL}; font-weight: 600; \
                                 cursor: pointer; border: 1px solid {}; background: {}; color: {};",
                                if on { FOCUS_FG } else { LINE },
                                if on { FOCUS_BG } else { "transparent" },
                                if on { FOCUS_FG } else { MUTED },
                            ),
                            onclick: move |_| on_pick.call(pick.clone()),
                            "{opt}"
                        }
                    }
                }
            }
        }
    }
}

// ── Menus ──────────────────────────────────────────────────────────────────

/// What a menu item does when chosen.
#[derive(Clone, PartialEq, Debug)]
pub enum ItemKind {
    /// Runs at once.
    Run,
    /// Asks for a name first (Rename…, Save as…, Duplicate…, New…).
    Name {
        initial: String,
        /// The confirm button's label.
        confirm: String,
        /// Names it may not take.
        taken: Vec<String>,
    },
    /// Deletes, on a second click.
    Delete,
    /// A heading over the items after it.
    Head,
    /// A line between groups.
    Sep,
}

/// One item of a menu. `id` is what the owner's `on_pick` receives.
#[derive(Clone, PartialEq, Debug)]
pub struct MenuItem {
    pub id: &'static str,
    pub label: String,
    pub kind: ItemKind,
    /// Why it cannot be chosen now; shown disabled with the reason.
    pub disabled: Option<String>,
    /// A ✓ beside it.
    pub checked: bool,
}

impl MenuItem {
    #[must_use]
    pub fn run(id: &'static str, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            kind: ItemKind::Run,
            disabled: None,
            checked: false,
        }
    }

    /// An item that asks for a name, starting from `initial`.
    #[must_use]
    pub fn name(
        id: &'static str,
        label: impl Into<String>,
        confirm: impl Into<String>,
        initial: impl Into<String>,
        taken: Vec<String>,
    ) -> Self {
        Self {
            kind: ItemKind::Name {
                initial: initial.into(),
                confirm: confirm.into(),
                taken,
            },
            ..Self::run(id, label)
        }
    }

    /// Delete — refused (disabled, with the reason) when `refused` is set.
    #[must_use]
    pub fn delete(id: &'static str, label: impl Into<String>, refused: Option<String>) -> Self {
        Self {
            kind: ItemKind::Delete,
            disabled: refused,
            ..Self::run(id, label)
        }
    }

    #[must_use]
    pub fn head(label: impl Into<String>) -> Self {
        Self {
            kind: ItemKind::Head,
            ..Self::run("", label)
        }
    }

    #[must_use]
    pub fn sep() -> Self {
        Self {
            kind: ItemKind::Sep,
            ..Self::run("", "")
        }
    }

    /// Disabled, for `why`, when `why` is `Some`.
    #[must_use]
    pub fn unless(mut self, why: Option<String>) -> Self {
        if why.is_some() {
            self.disabled = why;
        }
        self
    }
}

/// A chosen item: its `id`, and the name typed for a naming item (else
/// empty).
#[derive(Clone, PartialEq, Debug)]
pub struct Picked {
    pub id: &'static str,
    pub text: String,
}

/// A menu's width, px — the ⋯ button right-aligns it under itself.
const MENU_W: f64 = 236.0;

/// Show `items` at client `(x, y)` through the host.
fn open_menu(host: PopupHost, x: f64, y: f64, items: Vec<MenuItem>, on_pick: EventHandler<Picked>, on_closed: impl Fn() + 'static) {
    host.open(
        x,
        y,
        MENU_W,
        move || {
            rsx! {
                MenuPanel {
                    items: items.clone(),
                    on_pick,
                    on_close: move |()| {
                        // After the click is done with the row (see PopupLayer).
                        spawn(async move { host.close() });
                    },
                }
            }
        },
        on_closed,
    );
}

/// The top-left of the element a mouse event hit, in client coordinates.
fn origin_of(e: &MouseEvent) -> (f64, f64) {
    let (c, el) = (e.client_coordinates(), e.element_coordinates());
    (c.x - el.x, c.y - el.y)
}

/// Open `items` as a context menu at the pointer (a right-click on a row).
/// `host` is captured at render — looking it up here would be a hook
/// called from an event.
pub fn context_menu(host: Option<PopupHost>, e: &MouseEvent, items: Vec<MenuItem>, on_pick: EventHandler<Picked>) {
    if items.is_empty() {
        return;
    }
    if let Some(h) = host {
        let c = e.client_coordinates();
        open_menu(h, c.x, c.y, items, on_pick, || {});
    }
}

/// Three dots, drawn — the font has no ⋯ glyph.
#[component]
fn DotsGlyph(#[props(default = 14)] size: u32) -> Element {
    rsx! {
        svg {
            width: "{size}", height: "{size}", view_box: "0 0 24 24", fill: "currentColor",
            style: "display: block; flex-shrink: 0; width: {size}px; height: {size}px;",
            circle { cx: "5", cy: "12", r: "2" }
            circle { cx: "12", cy: "12", r: "2" }
            circle { cx: "19", cy: "12", r: "2" }
        }
    }
}

/// The ⋯ button and its menu.
#[component]
pub fn ActionMenu(
    items: Vec<MenuItem>,
    on_pick: EventHandler<Picked>,
    #[props(default = "More".to_string())] title: String,
    /// The button's side, px.
    #[props(default = 22)]
    size: u32,
    /// Drawn without a border — inside a bar that has one.
    #[props(default)]
    bare: bool,
) -> Element {
    // Every hook first: `use_context` inside the click handler would be a
    // hook called outside render.
    let host = PopupHost::try_use();
    let mut open = use_signal(|| false);
    let border = if bare { "transparent" } else { LINE };
    let (fg, bg) = if open() { (FOCUS_FG, FOCUS_BG) } else { (MUTED, "transparent") };
    let glyph = (size * 3 / 5).max(10);
    rsx! {
        div { style: "position: relative; display: flex; flex-shrink: 0;",
            button {
                style: "display: flex; align-items: center; justify-content: center; width: {size}px; \
                        height: {size}px; padding: 0; border-radius: {R_SM}; border: 1px solid {border}; \
                        background: {bg}; color: {fg}; cursor: pointer;",
                title: "{title}",
                // The board's panels are draggable; a press here is not a drag.
                onpointerdown: move |e: PointerEvent| e.stop_propagation(),
                onclick: {
                    let items = items.clone();
                    move |e: MouseEvent| {
                        e.stop_propagation();
                        if open() {
                            open.set(false);
                            if let Some(h) = host {
                                h.close();
                            }
                            return;
                        }
                        open.set(true);
                        let Some(h) = host else { return };
                        let (x, y) = origin_of(&e);
                        open_menu(
                            h,
                            x + f64::from(size) - MENU_W,
                            y + f64::from(size) + 4.0,
                            items.clone(),
                            on_pick,
                            move || {
                                let mut o = open;
                                o.set(false);
                            },
                        );
                    }
                },
                DotsGlyph { size: glyph }
            }
            if open() && host.is_none() {
                div { style: "position: absolute; top: calc(100% + 4px); right: 0; z-index: 300; width: {MENU_W}px;",
                    MenuPanel { items: items.clone(), on_pick, on_close: move |()| open.set(false) }
                }
            }
        }
    }
}

/// The menu's contents: its items, or — once a naming item is chosen — the
/// name field in their place.
#[component]
pub fn MenuPanel(items: Vec<MenuItem>, on_pick: EventHandler<Picked>, on_close: EventHandler<()>) -> Element {
    let mut naming = use_signal(|| None::<usize>);
    let mut armed = use_signal(|| None::<usize>);
    let row = format!(
        "display: flex; align-items: center; gap: 8px; padding: 6px 9px; border-radius: {R_SM}; \
         font-size: {T_SMALL}; white-space: nowrap; overflow: hidden;"
    );
    let head = "padding: 6px 9px 3px; font-size: 9px; letter-spacing: 0.12em; text-transform: uppercase; \
                color: #71717a; white-space: nowrap; overflow: hidden;";
    let panel = format!(
        "display: flex; flex-direction: column; gap: 1px; padding: 4px; min-width: {MENU_W}px; \
         max-width: 320px; border: 1px solid {LINE_STRONG}; border-radius: {R_MD}; background: {MENU}; \
         box-shadow: 0 12px 32px #000c;"
    );

    if let Some(i) = naming() {
        if let Some(MenuItem { id, label, kind: ItemKind::Name { initial, confirm, taken }, .. }) = items.get(i).cloned() {
            return rsx! {
                div { style: "{panel} padding: 8px; gap: 6px;",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    div { style: "{head} padding: 0 1px;", "{label.trim_end_matches('…')}" }
                    NamePrompt {
                        label: confirm,
                        initial,
                        taken,
                        on_done: move |name: Option<String>| {
                            if let Some(text) = name {
                                on_pick.call(Picked { id, text });
                            }
                            on_close.call(());
                        },
                    }
                }
            };
        }
    }

    rsx! {
        div { style: "{panel}",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            onkeydown: move |e: KeyboardEvent| {
                if e.key() == Key::Escape {
                    on_close.call(());
                }
            },
            for (i, item) in items.iter().cloned().enumerate() {
                match item.kind.clone() {
                    ItemKind::Sep => rsx! {
                        div { key: "{i}", style: "height: 1px; margin: 3px 4px; background: {LINE};" }
                    },
                    ItemKind::Head => rsx! {
                        div { key: "{i}", style: "{head}", "{item.label}" }
                    },
                    kind => {
                        let id = item.id;
                        let checked = item.checked;
                        let tip = item.disabled.clone().unwrap_or_default();
                        let off = item.disabled.is_some();
                        let danger = kind == ItemKind::Delete;
                        let is_armed = armed() == Some(i);
                        let colour = if off { DIM } else if danger { DANGER } else { "#d4d4d8" };
                        let bg = if is_armed { DANGER } else { "transparent" };
                        let fg = if is_armed { DANGER_INK } else { colour };
                        let cursor = if off { "default" } else { "pointer" };
                        let label = if is_armed { "Click again to delete".to_string() } else { item.label.clone() };
                        let why = item.disabled.clone().filter(|_| danger);
                        rsx! {
                            div { key: "{i}", style: "display: contents;",
                            div {
                                class: if off || is_armed { "" } else { "hover:bg-accent/40" },
                                style: "{row} color: {fg}; background: {bg}; cursor: {cursor};",
                                title: "{tip}",
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    if off {
                                        return;
                                    }
                                    match &kind {
                                        ItemKind::Name { .. } => naming.set(Some(i)),
                                        ItemKind::Delete if armed() != Some(i) => armed.set(Some(i)),
                                        _ => {
                                            on_pick.call(Picked { id, text: String::new() });
                                            on_close.call(());
                                        }
                                    }
                                },
                                span { style: "width: 12px; flex-shrink: 0; text-align: center; color: {LIVE};",
                                    if checked { "✓" } else { "" }
                                }
                                span { style: "flex: 1 1 0; min-width: 0; overflow: hidden;", "{label}" }
                            }
                            if let Some(why) = why {
                                div {
                                    style: "padding: 0 9px 5px 29px; font-size: {T_META}; line-height: 1.4; color: {FAINT}; \
                                            white-space: normal;",
                                    "{why}"
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
    let (title_size, pad) = if small { ("12px", "5px 8px") } else { ("13px", "7px 8px") };
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
pub fn PickList(options: Vec<PickOption>, on_pick: EventHandler<usize>, on_close: EventHandler<()>) -> Element {
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
    #[props(default)] placeholder: String,
    /// Extra inline style for the bar (width, flex).
    #[props(default)]
    style: String,
) -> Element {
    let host = PopupHost::try_use();
    let mut open = use_signal(|| false);
    // Compact fills its row (22–30px on the control surface); the list drops
    // below whichever it is.
    let (h, height): (u32, &str) = if compact { (30, "100%") } else { (54, "54px") };
    let side = if compact { 18 } else { 26 };
    let (eyebrow, title) = if compact { ("8px", "10px") } else { ("9px", "13px") };
    let (radius, name_pad, menu_pad) = if compact { ("4px", "5px", "0px") } else { ("8px", "9px", "3px") };
    let drop_bg = if open() { FOCUS_BG } else { "transparent" };
    let empty = if placeholder.is_empty() { "—".to_string() } else { placeholder.clone() };
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
                        height: 100%; flex-shrink: 0; padding: 0; border: none; border-left: 1px solid {LINE}; \
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
                    border: 1px solid {LINE_STRONG}; border-radius: {radius}; \
                    background: {FIELD}; overflow: visible; {style}",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            // ▾ — the list.
            button {
                style: "display: flex; align-items: center; justify-content: center; width: {side}px; \
                        flex-shrink: 0; padding: 0; border: none; border-right: 1px solid {LINE}; \
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
                              border-left: 1px solid {LINE}; padding: 0 {menu_pad};",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_must_be_new_and_not_blank() {
        let taken = vec!["Clean".to_string(), "Lead ".to_string()];
        assert!(name_problem("  ", &taken).is_some());
        assert!(name_problem("clean", &taken).is_some(), "case-insensitive");
        assert!(name_problem("lead", &taken).is_some(), "trimmed");
        assert!(name_problem("Crunch", &taken).is_none());
    }

    #[test]
    fn a_disabled_item_keeps_its_reason() {
        let d = MenuItem::delete("del", "Delete", Some("In use".into()));
        assert_eq!(d.disabled.as_deref(), Some("In use"));
        assert_eq!(MenuItem::run("x", "X").unless(None).disabled, None);
    }
}
