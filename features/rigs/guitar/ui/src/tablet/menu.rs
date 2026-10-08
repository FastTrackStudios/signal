//! The touch menu — the prototype's `ui/Menu.tsx`: items are data — run
//! one, name something in place, delete in two taps — and an item the rig
//! would refuse is shown disabled with the reason. Opened from a ⋯ button
//! (`MoreButton`) under itself, or at a point; drawn through the app's
//! popup layer, 300pt wide with 48pt rows.

use dioxus::prelude::*;
use signal_widgets::PopupHost;

use super::tokens::*;

/// The menu's width, pt.
pub const MENU_W: f64 = 300.0;

#[derive(Clone, PartialEq, Debug)]
pub enum Item {
    Head(String),
    Sep,
    Run { id: String, label: String, detail: String, disabled: Option<String>, checked: bool },
    Name { id: String, label: String, initial: String, confirm: String, taken: Vec<String> },
    Delete { id: String, label: String, disabled: Option<String> },
}

impl Item {
    pub fn head(label: impl Into<String>) -> Self {
        Self::Head(label.into())
    }
    pub fn run(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self::Run { id: id.into(), label: label.into(), detail: String::new(), disabled: None, checked: false }
    }
    pub fn name(id: impl Into<String>, label: impl Into<String>, initial: impl Into<String>, confirm: impl Into<String>, taken: Vec<String>) -> Self {
        Self::Name { id: id.into(), label: label.into(), initial: initial.into(), confirm: confirm.into(), taken }
    }
    pub fn delete(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self::Delete { id: id.into(), label: label.into(), disabled: None }
    }
    #[must_use]
    pub fn detail(mut self, d: impl Into<String>) -> Self {
        if let Self::Run { detail, .. } = &mut self {
            *detail = d.into();
        }
        self
    }
    #[must_use]
    pub fn checked(mut self, on: bool) -> Self {
        if let Self::Run { checked, .. } = &mut self {
            *checked = on;
        }
        self
    }
    /// Disabled, for `why`, when `why` is `Some`.
    #[must_use]
    pub fn unless(mut self, why: Option<String>) -> Self {
        match &mut self {
            Self::Run { disabled, .. } | Self::Delete { disabled, .. } => *disabled = why,
            _ => {}
        }
        self
    }
}

/// A chosen item: its id, and the name typed for a naming item.
#[derive(Clone, PartialEq, Debug)]
pub struct Picked {
    pub id: String,
    pub text: String,
}

/// Open `items` with the menu's top-left at client `(x, y)`.
pub fn open_menu(host: Option<PopupHost>, x: f64, y: f64, items: Vec<Item>, on_pick: EventHandler<Picked>) {
    let Some(host) = host else { return };
    if items.is_empty() {
        return;
    }
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
                        spawn(async move { host.close() });
                    },
                }
            }
        },
        || {},
    );
}

/// The ⋯ glyph, drawn, 48pt — and the menu under itself.
#[component]
pub fn MoreButton(label: String, items: Vec<Item>, on_pick: EventHandler<Picked>) -> Element {
    let host = PopupHost::try_use();
    rsx! {
        button {
            "aria-label": "{label}",
            title: "{label}",
            style: "width: 48px; height: 48px; flex-shrink: 0; display: flex; align-items: center; justify-content: center; border: none; border-radius: {R}; background: transparent; color: {INK_2}; cursor: pointer; padding: 0;",
            onpointerdown: move |e: PointerEvent| e.stop_propagation(),
            onclick: move |e: MouseEvent| {
                e.stop_propagation();
                let (c, el) = (e.client_coordinates(), e.element_coordinates());
                let (left, top) = (c.x - el.x, c.y - el.y);
                open_menu(host, left + 48.0 - MENU_W, top + 52.0, items.clone(), on_pick);
            },
            svg { width: "20", height: "4", view_box: "0 0 20 4", style: "display: block;",
                circle { cx: "2", cy: "2", r: "2", fill: INK_2 }
                circle { cx: "10", cy: "2", r: "2", fill: INK_2 }
                circle { cx: "18", cy: "2", r: "2", fill: INK_2 }
            }
        }
    }
}

/// The menu's contents: its items, or — once a naming item is chosen — the
/// name field in their place.
#[component]
fn MenuPanel(items: Vec<Item>, on_pick: EventHandler<Picked>, on_close: EventHandler<()>) -> Element {
    let mut naming = use_signal(|| None::<usize>);
    let mut armed = use_signal(|| None::<usize>);
    let mut text = use_signal(String::new);
    let panel = format!(
        "width: {MENU_W}px; max-height: 70vh; overflow-y: auto; padding: 4px; box-sizing: border-box; background: #0d0d10; border: 1px solid {RULE_STRONG}; border-radius: {R_MD}; box-shadow: 0 16px 40px rgba(0,0,0,0.7); font-family: {FONT}; color: {INK};"
    );
    if let Some(i) = naming()
        && let Some(Item::Name { id, label, initial, confirm, taken }) = items.get(i).cloned()
    {
        let typed = text();
        let clash = taken.iter().any(|t| t.eq_ignore_ascii_case(typed.trim()) && *t != initial);
        let ok = !typed.trim().is_empty() && !clash;
        let title = label.trim_end_matches('…').to_string();
        let commit = {
            let id = id.clone();
            move || {
                let t = text().trim().to_string();
                if t.is_empty() {
                    return;
                }
                on_pick.call(Picked { id: id.clone(), text: t });
                on_close.call(());
            }
        };
        let commit2 = commit.clone();
        return rsx! {
            div { style: "{panel}",
                div { style: "padding: 10px; display: flex; flex-direction: column; gap: 10px;",
                    div { style: "font-size: 12px; font-weight: 700; letter-spacing: 0.1em; text-transform: uppercase; color: {INK_3};", "{title}" }
                    input {
                        value: "{typed}",
                        autofocus: true,
                        style: "min-height: 48px; padding: 0 12px; border: 1px solid {RULE_STRONG}; border-radius: {R}; background: #0a0a0d; color: {INK}; font-size: 17px; font-family: {FONT}; box-sizing: border-box;",
                        oninput: move |e| text.set(e.value()),
                        onkeydown: move |e: KeyboardEvent| {
                            if e.key() == Key::Enter && ok {
                                commit();
                            }
                        },
                    }
                    if clash {
                        div { style: "color: {VOID}; font-size: 14px;", "That name is taken." }
                    }
                    div { style: "display: flex; gap: 8px; justify-content: flex-end;",
                        button {
                            style: "min-height: 44px; padding: 0 16px; border-radius: {R}; border: 1px solid {RULE_STRONG}; background: transparent; color: {INK}; font-weight: 600; font-size: 15px; font-family: {FONT}; cursor: pointer;",
                            onclick: move |_| on_close.call(()),
                            "Cancel"
                        }
                        button {
                            disabled: !ok,
                            style: "min-height: 44px; padding: 0 18px; border-radius: {R}; border: 1px solid {pick(ok, DIM, RULE_STRONG)}; background: {pick(ok, DIM, CLEAR)}; color: {pick(ok, INK, INK_3)}; font-weight: 650; font-size: 15px; font-family: {FONT}; cursor: pointer;",
                            onclick: move |_| if ok { commit2() },
                            "{confirm}"
                        }
                    }
                }
            }
        };
    }
    rsx! {
        div { role: "menu", style: "{panel}",
            for (i, it) in items.iter().cloned().enumerate() {
                match it {
                    Item::Sep => rsx! { div { key: "{i}", style: "height: 1px; margin: 4px 6px; background: {RULE};" } },
                    Item::Head(label) => rsx! {
                        div { key: "{i}", style: "padding: 10px 14px 4px; font-size: 12px; font-weight: 700; letter-spacing: 0.1em; text-transform: uppercase; color: {INK_3}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{label}" }
                    },
                    Item::Run { id, label, detail, disabled, checked } => {
                        let off = disabled.is_some();
                        rsx! {
                            div { key: "{i}",
                                button {
                                    disabled: off,
                                    style: "{row(false)} color: {pick(off, INK_3, INK)};",
                                    onclick: move |_| {
                                        on_pick.call(Picked { id: id.clone(), text: String::new() });
                                        on_close.call(());
                                    },
                                    span { style: "width: 14px; display: flex; justify-content: center;",
                                        if checked { span { style: "width: 8px; height: 8px; border-radius: 999px; background: {LIVE};" } }
                                    }
                                    span { style: "flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis;", "{label}" }
                                    if !detail.is_empty() { span { style: "font-size: 14px; color: {INK_3}; font-weight: 500;", "{detail}" } }
                                }
                                if let Some(why) = disabled {
                                    div { style: "padding: 0 14px 8px 38px; font-size: 13px; color: {INK_3}; line-height: 1.35;", "{why}" }
                                }
                            }
                        }
                    }
                    Item::Name { label, initial, .. } => rsx! {
                        div { key: "{i}",
                            button {
                                style: "{row(false)} color: {INK};",
                                onclick: move |_| {
                                    text.set(initial.clone());
                                    naming.set(Some(i));
                                },
                                span { style: "width: 14px;" }
                                span { style: "flex: 1; min-width: 0;", "{label}" }
                            }
                        }
                    },
                    Item::Delete { id, label, disabled } => {
                        let off = disabled.is_some();
                        let is_armed = armed() == Some(i);
                        rsx! {
                            div { key: "{i}",
                                button {
                                    disabled: off,
                                    style: "{row(is_armed)} color: {pick(is_armed, DANGER_INK, pick(off, INK_3, VOID))};",
                                    onclick: move |_| {
                                        if is_armed {
                                            on_pick.call(Picked { id: id.clone(), text: String::new() });
                                            on_close.call(());
                                        } else {
                                            armed.set(Some(i));
                                        }
                                    },
                                    span { style: "width: 14px;" }
                                    span { style: "flex: 1; min-width: 0;", if is_armed { "Tap again to delete" } else { "{label}" } }
                                }
                                if let Some(why) = disabled {
                                    div { style: "padding: 0 14px 8px 38px; font-size: 13px; color: {INK_3}; line-height: 1.35;", "{why}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn row(armed: bool) -> String {
    format!(
        "display: flex; align-items: center; gap: 10px; width: 100%; min-height: 48px; padding: 0 14px; border: none; border-radius: {R}; text-align: left; font-size: 16px; font-weight: 560; font-family: {FONT}; cursor: pointer; background: {};",
        if armed { VOID } else { CLEAR }
    )
}
