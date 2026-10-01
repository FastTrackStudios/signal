//! **The sound browser** — how every rig loads a sound. One component, so a
//! guitar module preset and a keys layer preset are found, heard, loaded and
//! kept the same way.
//!
//! It is docked beside the rig and follows the selection: the rig hands it
//! the **scopes** the selection can hold (a keys lane: layer presets, module
//! presets; a guitar block: its block presets) and the **entries** of the
//! current one, and says where a load lands ("→ Keys 1 · module A"). The ⤢
//! button opens the same browser out full size — a rail of scopes, the list,
//! and a detail pane with the entry's actions.
//!
//! | gesture | does |
//! |---|---|
//! | click a row | loads it |
//! | ↑ ↓ in the search field | **auditions**: the highlighted sound plays, live |
//! | Enter · **Keep** | keeps what is auditioning (or loads the highlighted row) |
//! | Esc · **Undo** | puts back what played before the audition began |
//! | ⋯ / right-click on a row | its actions (rename, duplicate, delete…) |
//! | ⋯ in the header | the level's actions (save as a preset…) |
//!
//! By touch (see [`crate::touch`]) a tap **auditions** instead of loading
//! outright — the sound plays, and the bar's Keep / Undo decide; tapping the
//! same row again keeps it, and leaving keeps it too. A long-press on a row
//! opens its menu, and every button is a fingertip wide.
//!
//! Entries are data (see [`BrowseEntry`]); every action comes back through a
//! handful of handlers, so the rig decides what "load", "audition" and
//! "keep" mean for it. Searching is the rig's too — it owns the query and
//! filters (the keys library is ~41k entries, which must never be a prop),
//! then hands over at most a screenful plus how many matched.

use dioxus::prelude::*;

use crate::PopupHost;
use crate::kit::{ActionMenu, Dot, MenuItem, MenuPanel, Picked, context_menu};
use crate::theme::{
    EYEBROW, FAINT, FIELD, FOCUS_BG, FOCUS_FG, LINE, LINE_STRONG, MUTED, PANE, R_MD, R_SM, SIDEBAR,
    T_BODY, T_META, T_SMALL, TEXT,
};

/// One level the browser can list at: its label, and where a load lands.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct BrowseScope {
    pub id: String,
    pub label: String,
    /// Where a load lands, spelled out ("→ Keys 1 · module A"); empty when
    /// nothing is selected to load into.
    pub target: String,
}

/// One loadable sound.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct BrowseEntry {
    /// What the rig's handlers receive.
    pub id: String,
    pub name: String,
    /// The line under the name (a kind, a snapshot's contents).
    pub sub: String,
    /// A short note at the right (a count).
    pub note: String,
    /// Playing now.
    pub live: bool,
    /// Playing, with edits not saved into it.
    pub modified: bool,
    /// Saved by the player (not scanned from a library).
    pub user: bool,
    /// The heading it sits under; a change of group starts a new heading.
    pub group: String,
    /// Its ⋯ / right-click actions.
    pub menu: Vec<MenuItem>,
    /// Alternatives behind it — variations of a sound, snapshots of a
    /// preset — each loadable on its own.
    pub children: Vec<BrowseEntry>,
}

/// A filter chip under the search field.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct BrowseChip {
    pub id: String,
    pub label: String,
    pub on: bool,
}

/// Everything the browser shows, as one value — the docked browser and its
/// full-size sheet both read it (the sheet through a signal, so it follows
/// the rig while it is open).
#[derive(Clone, PartialEq, Debug, Default)]
struct Model {
    scopes: Vec<BrowseScope>,
    scope: String,
    entries: Vec<BrowseEntry>,
    total: usize,
    menu: Vec<MenuItem>,
    chips: Vec<BrowseChip>,
    empty: String,
    accent: String,
    audition: bool,
}

impl Model {
    fn current(&self) -> Option<&BrowseScope> {
        self.scopes.iter().find(|s| s.id == self.scope)
    }

    /// Every loadable row in the order drawn: each entry, then its children.
    fn order(&self) -> Vec<&BrowseEntry> {
        self.entries
            .iter()
            .flat_map(|e| std::iter::once(e).chain(e.children.iter()))
            .collect()
    }

    fn find(&self, id: &str) -> Option<&BrowseEntry> {
        self.order().into_iter().find(|e| e.id == id)
    }

    /// The row `delta` rows from `from` (from the live row, or the top, when
    /// nothing is highlighted), clamped to the list.
    fn step(&self, from: Option<&str>, delta: i32) -> Option<String> {
        let order = self.order();
        if order.is_empty() {
            return None;
        }
        let here = from
            .and_then(|id| order.iter().position(|e| e.id == id))
            .or_else(|| order.iter().position(|e| e.live));
        let next = match here {
            Some(i) => (i as i64 + i64::from(delta)).clamp(0, order.len() as i64 - 1) as usize,
            None if delta < 0 => order.len() - 1,
            None => 0,
        };
        Some(order[next].id.clone())
    }
}

/// The handlers, as one `Copy` value the body and the sheet share.
#[derive(Clone, Copy)]
struct Handlers {
    query: Signal<String>,
    on_scope: EventHandler<String>,
    on_load: EventHandler<String>,
    on_preview: EventHandler<String>,
    on_audition_end: EventHandler<bool>,
    on_menu: EventHandler<Picked>,
    on_entry_menu: EventHandler<(String, Picked)>,
    on_chip: EventHandler<String>,
    art: Option<Callback<String, Element>>,
}

/// The browser's own state: the highlighted row and the audition.
#[derive(Clone, Copy)]
struct Ui {
    /// The highlighted row (arrows move it; the sheet's detail shows it).
    cursor: Signal<Option<String>>,
    /// An audition is running (the rig has something to put back).
    auditioning: Signal<bool>,
    /// The row the last preview loaded.
    previewed: Signal<Option<String>>,
    /// Bumped per arrow press; a preview only fires if it is still current,
    /// so holding an arrow does not load every sound it passes.
    tick: Signal<u64>,
}

/// How long the highlight must rest before its sound loads, ms.
const PREVIEW_MS: u64 = 160;

impl Ui {
    /// Leave the audition, keeping or undoing it.
    fn end(mut self, h: Handlers, keep: bool) {
        if *self.auditioning.peek() {
            self.auditioning.set(false);
            self.previewed.set(None);
            h.on_audition_end.call(keep);
        }
    }

    /// Load `id` for good (a click, or Enter on a row not already playing).
    fn load(mut self, h: Handlers, id: String) {
        let already = self.previewed.peek().as_deref() == Some(id.as_str());
        if !already {
            h.on_load.call(id.clone());
        }
        self.end(h, true);
        self.cursor.set(Some(id));
    }

    /// A tap on a row. With a mouse (or no audition) it loads. By touch it
    /// auditions — the tap is the finger's arrow key — and a second tap on
    /// the same row keeps it.
    fn tap(mut self, h: Handlers, audition: bool, touch: bool, id: String) {
        if !(touch && audition) {
            self.load(h, id);
            return;
        }
        if self.previewed.peek().as_deref() == Some(id.as_str()) {
            self.end(h, true);
            return;
        }
        self.tick += 1;
        self.cursor.set(Some(id.clone()));
        self.auditioning.set(true);
        self.previewed.set(Some(id.clone()));
        h.on_preview.call(id);
    }

    /// Move the highlight and, when the rig auditions, play it shortly.
    fn arrow(mut self, h: Handlers, model: &Model, delta: i32) {
        let from = self.cursor.peek().clone();
        let Some(id) = model.step(from.as_deref(), delta) else {
            return;
        };
        self.cursor.set(Some(id.clone()));
        if !model.audition {
            return;
        }
        let g = *self.tick.peek() + 1;
        self.tick.set(g);
        spawn(async move {
            architect::platform::sleep(std::time::Duration::from_millis(PREVIEW_MS)).await;
            if *self.tick.peek() != g {
                return;
            }
            self.auditioning.set(true);
            self.previewed.set(Some(id.clone()));
            h.on_preview.call(id);
        });
    }

    /// The search field's keys.
    fn key(mut self, h: Handlers, model: &Model, e: &KeyboardEvent) -> bool {
        match e.key() {
            Key::ArrowDown => self.arrow(h, model, 1),
            Key::ArrowUp => self.arrow(h, model, -1),
            Key::Enter => {
                if *self.auditioning.peek() {
                    self.tick += 1;
                    let cursor = self.cursor.peek().clone();
                    match cursor {
                        Some(id) => self.load(h, id),
                        None => self.end(h, true),
                    }
                } else if let Some(id) = self.cursor.peek().clone() {
                    self.load(h, id);
                } else if let Some(first) = model.order().first() {
                    self.load(h, first.id.clone());
                }
            }
            Key::Escape => {
                if *self.auditioning.peek() {
                    self.tick += 1;
                    self.end(h, false);
                    self.cursor.set(None);
                } else {
                    return false;
                }
            }
            _ => return false,
        }
        true
    }
}

/// The docked browser.
///
/// `entries` is what to draw (the rig filters by [`query`](Self) and caps
/// it); `total` is how many matched before the cap.
#[component]
pub fn SoundBrowser(
    /// The levels the selection can hold, and the one showing.
    scopes: Vec<BrowseScope>,
    scope: String,
    on_scope: EventHandler<String>,
    entries: Vec<BrowseEntry>,
    #[props(default)] total: usize,
    /// The search text — the rig's, so its filter runs where the data is.
    query: Signal<String>,
    on_load: EventHandler<String>,
    /// Auditioning is on: arrows play the highlighted row via `on_preview`,
    /// and `on_audition_end(keep)` ends it.
    #[props(default)]
    audition: bool,
    #[props(default)] on_preview: EventHandler<String>,
    #[props(default)] on_audition_end: EventHandler<bool>,
    /// The level's own actions (the header's ⋯).
    #[props(default)]
    menu: Vec<MenuItem>,
    #[props(default)] on_menu: EventHandler<Picked>,
    #[props(default)] on_entry_menu: EventHandler<(String, Picked)>,
    #[props(default)] chips: Vec<BrowseChip>,
    #[props(default)] on_chip: EventHandler<String>,
    /// A picture at a row's right, by entry id (the guitar rig's preset
    /// looks).
    #[props(default)]
    art: Option<Callback<String, Element>>,
    /// Shown when there is nothing to list.
    #[props(default)]
    empty: String,
    /// The colour of "where it lands" and the highlight.
    #[props(default = FOCUS_FG.to_string())]
    accent: String,
    #[props(default = "272px".to_string())] width: String,
    /// Docked on the right (its border on the left).
    #[props(default)]
    right: bool,
    /// A close button in the header (a sidebar that can be put away).
    #[props(default)]
    on_close: Option<EventHandler<()>>,
    /// The rig's own pieces between the header and the search (what plays,
    /// its save/revert).
    #[props(default)]
    children: Element,
) -> Element {
    let model_now = Model {
        scopes,
        scope,
        entries,
        total,
        menu,
        chips,
        empty,
        accent,
        audition,
    };
    let mut model = use_signal(|| model_now.clone());
    if *model.peek() != model_now {
        model.set(model_now);
    }
    let h = Handlers {
        query,
        on_scope,
        on_load,
        on_preview,
        on_audition_end,
        on_menu,
        on_entry_menu,
        on_chip,
        art,
    };
    let ui = Ui {
        cursor: use_signal(|| None),
        auditioning: use_signal(|| false),
        previewed: use_signal(|| None),
        tick: use_signal(|| 0),
    };
    let host = PopupHost::try_use();
    // Leaving the level (or the browser) keeps whatever is auditioning.
    let scope_now = model.read().scope.clone();
    let mut seen_scope = use_signal(|| scope_now.clone());
    if *seen_scope.peek() != scope_now {
        seen_scope.set(scope_now);
        ui.end(h, true);
        let mut c = ui.cursor;
        c.set(None);
    }
    // Unmounted mid-audition (the rig closed, the selection went): keep it.
    use_drop(move || {
        if ui.auditioning.try_peek().is_ok_and(|a| *a) {
            h.on_audition_end.call(true);
        }
    });

    let expand = move |()| {
        let Some(host) = host else { return };
        host.open_sheet(
            move || rsx! { Body { model, h, ui, wide: true, on_close: move |()| host.close() } },
            || {},
        );
    };
    let border = if right { "border-left" } else { "border-right" };
    rsx! {
        div {
            style: "width: {width}; flex-shrink: 0; display: flex; flex-direction: column; min-height: 0; \
                    {border}: 1px solid {LINE}; background: {SIDEBAR}; color: {TEXT};",
            Body {
                model,
                h,
                ui,
                wide: false,
                on_expand: if host.is_some() { Some(EventHandler::new(expand)) } else { None },
                on_close,
                {children}
            }
        }
    }
}

/// The browser's contents, docked (`wide: false`) or full size.
#[component]
fn Body(
    model: Signal<Model>,
    h: Handlers,
    ui: Ui,
    wide: bool,
    #[props(default)] on_expand: Option<EventHandler<()>>,
    #[props(default)] on_close: Option<EventHandler<()>>,
    /// The rig's pieces under the header (docked only).
    children: Element,
) -> Element {
    let m = model.read().clone();
    let mut query = h.query;
    let current = m.current().cloned().unwrap_or_default();
    let count = if m.total > m.entries.len() {
        format!("{}+", m.entries.len())
    } else {
        m.order().len().to_string()
    };
    let header = rsx! {
        div { style: "display: flex; flex-direction: column; gap: 8px; padding: 10px 12px; \
                      border-bottom: 1px solid {LINE}; flex-shrink: 0;",
            div { style: "display: flex; align-items: center; gap: 6px; min-width: 0;",
                if wide || m.scopes.len() < 2 {
                    span { style: "{EYEBROW}", "{current.label}" }
                } else {
                    crate::Picker {
                        options: m.scopes.iter().map(|s| s.label.clone()).collect::<Vec<_>>(),
                        selected: m.scopes.iter().position(|s| s.id == m.scope).unwrap_or(0) as u32,
                        on_select: {
                            let ids: Vec<String> = m.scopes.iter().map(|s| s.id.clone()).collect();
                            move |i: u32| {
                                if let Some(id) = ids.get(i as usize) {
                                    h.on_scope.call(id.clone());
                                }
                            }
                        },
                    }
                }
                div { style: "flex: 1 1 0;" }
                span { style: "font-size: {T_META}; color: {FAINT};", "{count}" }
                if let Some(expand) = on_expand {
                    GlyphButton { title: "Open full size", onclick: move |()| expand.call(()), ExpandGlyph {} }
                }
                if let Some(close) = on_close {
                    GlyphButton { title: "Close (Esc)", onclick: move |()| close.call(()), CloseGlyph {} }
                }
                if !m.menu.is_empty() {
                    ActionMenu { items: m.menu.clone(), on_pick: h.on_menu, size: 22, title: "Actions" }
                }
            }
            if !current.target.is_empty() {
                span { style: "font-size: {T_SMALL}; color: {m.accent}; overflow: hidden; white-space: nowrap;",
                    "{current.target}"
                }
            }
            {children}
            input {
                style: "background: {FIELD}; border: 1px solid {LINE_STRONG}; border-radius: {R_SM}; \
                        padding: 7px 9px; color: {TEXT}; font-size: {T_BODY}; outline: none;",
                placeholder: if m.audition { "Search · ↑↓ to audition" } else { "Search" },
                value: "{query}",
                onmounted: move |e| {
                    if wide {
                        spawn(async move {
                            let _ = e.data().set_focus(true).await;
                        });
                    }
                },
                oninput: move |e| query.set(e.value()),
                onkeydown: {
                    let m = m.clone();
                    move |e: KeyboardEvent| {
                        if ui.key(h, &m, &e) {
                            e.prevent_default();
                            e.stop_propagation();
                        } else if e.key() == Key::Escape
                            && let Some(close) = on_close
                        {
                            close.call(());
                        }
                    }
                },
            }
            if !m.chips.is_empty() {
                div { style: "display: flex; flex-wrap: wrap; gap: 5px;",
                    for chip in m.chips.iter().cloned() {
                        Chip { key: "{chip.id}", chip, on_chip: h.on_chip }
                    }
                }
            }
            if ui.auditioning.read().clone() {
                AuditionBar { accent: m.accent.clone(), h, ui }
            }
        }
    };
    let list = rsx! {
        List { model, h, ui }
    };
    if !wide {
        return rsx! { {header} {list} };
    }
    // Full size: scopes down the left, the list, the highlighted entry's
    // detail on the right.
    rsx! {
        div {
            style: "flex: 1 1 0; display: flex; min-width: 0; min-height: 0; border: 1px solid {LINE_STRONG}; \
                    border-radius: {R_MD}; background: {SIDEBAR}; color: {TEXT}; overflow: hidden; \
                    box-shadow: 0 24px 64px #000c;",
            if m.scopes.len() > 1 {
                div { style: "width: 190px; flex-shrink: 0; display: flex; flex-direction: column; gap: 2px; \
                              padding: 10px 8px; border-right: 1px solid {LINE}; background: {PANE};",
                    span { style: "{EYEBROW} padding: 4px 6px 8px;", "Browse" }
                    for s in m.scopes.iter().cloned() {
                        {
                            let on = s.id == m.scope;
                            let (bg, fg) = if on { (FOCUS_BG, FOCUS_FG) } else { ("transparent", MUTED) };
                            rsx! {
                                button {
                                    key: "{s.id}",
                                    style: "appearance: none; border: none; text-align: left; cursor: pointer; \
                                            padding: 7px 9px; border-radius: {R_SM}; font-size: {T_BODY}; \
                                            font-weight: 600; background: {bg}; color: {fg};",
                                    onclick: move |_| h.on_scope.call(s.id.clone()),
                                    "{s.label}"
                                }
                            }
                        }
                    }
                }
            }
            div { style: "flex: 1 1 0; min-width: 0; display: flex; flex-direction: column; min-height: 0;",
                {header}
                {list}
            }
            Detail { model, h, ui }
        }
    }
}

/// The list: group headings, rows and their children.
#[component]
fn List(model: Signal<Model>, h: Handlers, ui: Ui) -> Element {
    let m = model.read();
    let cursor = ui.cursor.read().clone();
    let mut last_group = String::new();
    let truncated = m.total > m.entries.len();
    rsx! {
        div { style: "flex: 1 1 0%; min-height: 0; overflow-y: scroll; padding: 6px; \
                      display: flex; flex-direction: column; gap: 1px;",
            if m.entries.is_empty() {
                span { style: "padding: 10px 6px; font-size: {T_SMALL}; color: {FAINT}; line-height: 1.5;",
                    if m.empty.is_empty() { "Nothing here." } else { "{m.empty}" }
                }
            }
            for entry in m.entries.iter().cloned() {
                {
                    let heading = (!entry.group.is_empty() && entry.group != last_group)
                        .then(|| entry.group.clone());
                    last_group = entry.group.clone();
                    rsx! {
                        div { key: "{entry.id}", style: "display: contents;",
                        if let Some(g) = heading {
                            div { style: "{EYEBROW} padding: 10px 6px 4px;", "{g}" }
                        }
                        Row {
                            entry: entry.clone(),
                            indent: false,
                            lit: cursor.as_deref() == Some(entry.id.as_str()),
                            accent: m.accent.clone(),
                            audition: m.audition,
                            h,
                            ui,
                        }
                        for child in entry.children.iter().cloned() {
                            Row {
                                key: "{child.id}",
                                lit: cursor.as_deref() == Some(child.id.as_str()),
                                entry: child,
                                indent: true,
                                accent: m.accent.clone(),
                                audition: m.audition,
                                h,
                                ui,
                            }
                        }
                        }
                    }
                }
            }
            if truncated {
                span { style: "padding: 10px 6px; font-size: {T_META}; color: {FAINT}; line-height: 1.5;",
                    "{m.total} match — the first {m.entries.len()} are shown; type to narrow it."
                }
            }
        }
    }
}

/// One row: status dot, name over its sub-line, a note, its picture, ⋯.
#[component]
fn Row(
    entry: BrowseEntry,
    indent: bool,
    lit: bool,
    accent: String,
    audition: bool,
    h: Handlers,
    ui: Ui,
) -> Element {
    let host = PopupHost::try_use();
    let touch = crate::touch::use_touch();
    let long = crate::touch::use_long_press();
    let (bg, fg) = if lit {
        (FOCUS_BG, TEXT)
    } else if entry.live {
        ("rgba(34,197,94,0.07)", TEXT)
    } else {
        ("transparent", "#d4d4d8")
    };
    let pad = match (indent, touch) {
        (true, false) => "5px 8px 5px 22px",
        (false, false) => "6px 8px",
        (true, true) => "10px 8px 10px 22px",
        (false, true) => "11px 8px",
    };
    let edge = if lit { accent.clone() } else { "transparent".to_string() };
    let size = if indent { T_SMALL } else { T_BODY };
    let id = entry.id.clone();
    let menu = entry.menu.clone();
    let on_pick = {
        let id = id.clone();
        EventHandler::new(move |p: Picked| h.on_entry_menu.call((id.clone(), p)))
    };
    rsx! {
        div {
            class: if lit { "" } else { "hover:bg-accent/40" },
            style: "display: flex; align-items: center; gap: 8px; padding: {pad}; border-radius: {R_SM}; \
                    background: {bg}; color: {fg}; cursor: pointer; min-width: 0; \
                    border-left: 2px solid {edge};",
            // Touch: a long-press opens the row's menu (its right-click).
            onpointerdown: {
                let menu = menu.clone();
                move |e: PointerEvent| {
                    let menu = menu.clone();
                    long.down(&e, move |(x, y)| crate::kit::context_menu_at(host, x, y, menu, on_pick));
                }
            },
            onpointermove: move |e: PointerEvent| long.moved(&e),
            onpointerup: move |_| long.cancel(),
            onpointercancel: move |_| long.cancel(),
            onpointerleave: move |_| long.cancel(),
            onclick: {
                let id = id.clone();
                move |_| {
                    if !long.fired() {
                        ui.tap(h, audition, touch, id.clone());
                    }
                }
            },
            oncontextmenu: {
                let menu = menu.clone();
                move |e: MouseEvent| {
                    e.prevent_default();
                    context_menu(host, &e, menu.clone(), on_pick);
                }
            },
            Dot { live: entry.live, modified: entry.modified, hollow: true }
            div { style: "flex: 1 1 0; min-width: 0; display: flex; flex-direction: column; gap: 1px;",
                span { style: "font-size: {size}; font-weight: 600; white-space: nowrap; overflow: hidden;",
                    "{entry.name}"
                }
                if !entry.sub.is_empty() {
                    span { style: "font-size: {T_META}; color: {FAINT}; white-space: nowrap; overflow: hidden;",
                        "{entry.sub}"
                    }
                }
            }
            if entry.user {
                span { style: "font-size: 9px; font-weight: 700; letter-spacing: 0.08em; color: {MUTED}; \
                               border: 1px solid {LINE_STRONG}; border-radius: 999px; padding: 1px 6px;",
                    "SAVED"
                }
            }
            if !entry.note.is_empty() {
                span { style: "font-size: {T_META}; color: {FAINT}; flex-shrink: 0;", "{entry.note}" }
            }
            if let Some(art) = h.art {
                {art.call(entry.id.clone())}
            }
            if !menu.is_empty() {
                ActionMenu { items: menu.clone(), on_pick, size: 20, bare: true, title: "Actions" }
            }
        }
    }
}

/// The full-size browser's right pane: the highlighted entry and what can be
/// done with it.
#[component]
fn Detail(model: Signal<Model>, h: Handlers, ui: Ui) -> Element {
    let m = model.read();
    let cursor = ui.cursor.read().clone();
    let entry = cursor
        .as_deref()
        .and_then(|id| m.find(id))
        .or_else(|| m.order().into_iter().find(|e| e.live))
        .cloned();
    let target = m.current().map(|s| s.target.clone()).unwrap_or_default();
    rsx! {
        div { style: "width: 300px; flex-shrink: 0; display: flex; flex-direction: column; gap: 10px; \
                      padding: 16px; border-left: 1px solid {LINE}; background: {PANE}; min-height: 0;",
            match entry {
                None => rsx! {
                    span { style: "font-size: {T_SMALL}; color: {FAINT}; line-height: 1.5;",
                        "Highlight a sound (↑ ↓) to see it here."
                    }
                },
                Some(e) => {
                    let id = e.id.clone();
                    rsx! {
                        div { style: "display: flex; align-items: center; gap: 8px;",
                            Dot { live: e.live, modified: e.modified, hollow: true, size: 8 }
                            span { style: "font-size: 15px; font-weight: 700; color: {TEXT};", "{e.name}" }
                        }
                        if !e.sub.is_empty() {
                            span { style: "font-size: {T_SMALL}; color: {MUTED}; line-height: 1.5;", "{e.sub}" }
                        }
                        if !e.group.is_empty() {
                            span { style: "font-size: {T_META}; color: {FAINT};", "{e.group}" }
                        }
                        if let Some(art) = h.art {
                            div { style: "display: flex;", {art.call(e.id.clone())} }
                        }
                        button {
                            style: "appearance: none; border: 1px solid #2563eb; background: #2563eb; color: #fff; \
                                    border-radius: {R_SM}; padding: 8px 12px; font-size: {T_BODY}; font-weight: 700; \
                                    cursor: pointer;",
                            disabled: target.is_empty(),
                            onclick: {
                                let id = id.clone();
                                move |_| ui.load(h, id.clone())
                            },
                            if e.live { "Playing" } else if target.is_empty() { "Select something to load into" } else { "Load {target}" }
                        }
                        if !e.menu.is_empty() {
                            MenuPanel {
                                items: e.menu.clone(),
                                on_pick: {
                                    let id = id.clone();
                                    move |p: Picked| h.on_entry_menu.call((id.clone(), p))
                                },
                                on_close: |()| {},
                            }
                        }
                    }
                }
            }
        }
    }
}

/// While an audition runs: what is playing on trial, and the two ways out —
/// the buttons a finger has for Enter and Esc.
#[component]
fn AuditionBar(accent: String, h: Handlers, ui: Ui) -> Element {
    let touch = crate::touch::use_touch();
    let pad = if touch { "11px 16px" } else { "4px 10px" };
    let btn = |primary: bool| {
        let (bg, fg, border) = if primary {
            ("#2563eb", "#ffffff", "#2563eb")
        } else {
            ("transparent", TEXT, LINE_STRONG)
        };
        format!(
            "appearance: none; cursor: pointer; border-radius: {R_SM}; padding: {pad}; font-size: {T_SMALL}; \
             font-weight: 700; background: {bg}; color: {fg}; border: 1px solid {border};"
        )
    };
    rsx! {
        div { style: "display: flex; align-items: center; gap: 6px;",
            span { style: "flex: 1 1 0; min-width: 0; font-size: {T_META}; color: {accent}; white-space: nowrap; overflow: hidden;",
                "Auditioning"
            }
            button {
                style: "{btn(false)}",
                title: "Put back what played before (Esc)",
                onclick: move |_| {
                    let mut u = ui;
                    u.tick += 1;
                    ui.end(h, false);
                    u.cursor.set(None);
                },
                "Undo"
            }
            button {
                style: "{btn(true)}",
                title: "Keep it (Enter)",
                onclick: move |_| ui.end(h, true),
                "Keep"
            }
        }
    }
}

#[component]
fn Chip(chip: BrowseChip, on_chip: EventHandler<String>) -> Element {
    let pad = if crate::touch::use_touch() { "9px 14px" } else { "2px 9px" };
    let (border, bg, fg) = if chip.on {
        ("#1f2b3a", FOCUS_BG, FOCUS_FG)
    } else {
        ("#26262b", "#131316", "#71717a")
    };
    rsx! {
        button {
            style: "appearance: none; border: 1px solid {border}; border-radius: 999px; padding: {pad}; \
                    cursor: pointer; font-size: 10px; font-weight: 700; background: {bg}; color: {fg};",
            onclick: move |_| on_chip.call(chip.id.clone()),
            "{chip.label}"
        }
    }
}

#[component]
fn GlyphButton(title: String, onclick: EventHandler<()>, children: Element) -> Element {
    let side = crate::touch::hit(crate::touch::use_touch(), 22);
    rsx! {
        button {
            style: "display: flex; align-items: center; justify-content: center; width: {side}px; height: {side}px; \
                    padding: 0; flex-shrink: 0; border-radius: {R_SM}; border: 1px solid {LINE}; \
                    background: transparent; color: {MUTED}; cursor: pointer;",
            title: "{title}",
            onclick: move |e: MouseEvent| {
                e.stop_propagation();
                onclick.call(());
            },
            {children}
        }
    }
}

/// Two corner arrows — the font has no ⤢.
#[component]
fn ExpandGlyph() -> Element {
    rsx! {
        svg { width: "12", height: "12", view_box: "0 0 24 24", fill: "none", stroke: "currentColor",
            stroke_width: "2.4", stroke_linecap: "round", stroke_linejoin: "round",
            style: "display: block; width: 12px; height: 12px;",
            path { d: "M14 4h6v6M20 4l-7 7M10 20H4v-6M4 20l7-7" }
        }
    }
}

#[component]
fn CloseGlyph() -> Element {
    rsx! {
        svg { width: "12", height: "12", view_box: "0 0 24 24", fill: "none", stroke: "currentColor",
            stroke_width: "2.4", stroke_linecap: "round",
            style: "display: block; width: 12px; height: 12px;",
            path { d: "M6 6l12 12M18 6L6 18" }
        }
    }
}

impl PartialEq for Handlers {
    fn eq(&self, other: &Self) -> bool {
        self.query == other.query
            && self.on_scope == other.on_scope
            && self.on_load == other.on_load
            && self.on_preview == other.on_preview
            && self.on_audition_end == other.on_audition_end
            && self.on_menu == other.on_menu
            && self.on_entry_menu == other.on_entry_menu
            && self.on_chip == other.on_chip
            && self.art == other.art
    }
}

impl PartialEq for Ui {
    fn eq(&self, other: &Self) -> bool {
        self.cursor == other.cursor
            && self.auditioning == other.auditioning
            && self.previewed == other.previewed
            && self.tick == other.tick
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(id: &str, live: bool, kids: &[&str]) -> BrowseEntry {
        BrowseEntry {
            id: id.into(),
            name: id.into(),
            live,
            children: kids.iter().map(|k| e(k, false, &[])).collect(),
            ..BrowseEntry::default()
        }
    }

    #[test]
    fn arrows_walk_rows_and_their_children_from_what_plays() {
        let m = Model {
            entries: vec![e("a", false, &["a1", "a2"]), e("b", true, &[]), e("c", false, &[])],
            ..Model::default()
        };
        // Nothing highlighted: start from the live row.
        assert_eq!(m.step(None, 1).as_deref(), Some("c"));
        assert_eq!(m.step(None, -1).as_deref(), Some("a2"));
        assert_eq!(m.step(Some("a"), 1).as_deref(), Some("a1"));
        // Clamped at both ends.
        assert_eq!(m.step(Some("c"), 1).as_deref(), Some("c"));
        assert_eq!(m.step(Some("a"), -1).as_deref(), Some("a"));
        let empty = Model::default();
        assert_eq!(empty.step(None, 1), None);
    }
}
