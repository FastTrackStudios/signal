//! **One app chrome** — an icon rail, one top bar, and a panel rail.
//!
//! The app used to stack a bar per level of nesting: the window bar (workspace
//! tabs, engines, settings, window controls), then a rig bar (`‹ Rigs`), then
//! the rig's own bar (name, mode tabs, meters), then a zoom bar (`‹ Back`,
//! crumbs, page tabs). Four bars, ~140px, and three of them answering the same
//! question — *where am I*.
//!
//! This crate is the answer to that question once:
//!
//! ```text
//! ┌──┬────────────────────────────────────────────────┬──┐
//! │⌂ │ Keys ▸ Keys A ▸ Module B   [Layer|Module|Edit] │▤ │
//! │▦ │                    ● ▮▮▮▯  ⚙  ─ □ ✕            │♪ │
//! │♫ ├────────────────────────────────────────────────┤⌗ │
//! │  │                  the view                      │  │
//! └──┴────────────────────────────────────────────────┴──┘
//! ```
//!
//! - **Left rail** — *place*: the workspaces, and a sub-rail for the level
//!   beneath (the rigs, under Signal). Never scrolls away, costs no height.
//! - **Top bar** — *position and state*: the crumb trail (each crumb a menu of
//!   its siblings, so "back" is just clicking the parent), the current view's
//!   page tabs, and its status readouts. One bar, always the same shape.
//! - **Right rail** — *panels*: the browser, the MIDI monitor, the routing
//!   tree, logs. Things that used to be page-modes become slide-outs you can
//!   keep open while playing.
//!
//! Everything is inline-styled: the rig UIs render through Blitz (VST/REAPER
//! parity), which does not reliably load external CSS, so the chrome they
//! share cannot depend on a stylesheet.
//!
//! ## How a view talks to the chrome
//!
//! The chrome is a set of signals in context. Each level of the app claims a
//! [`Depth`] and publishes into it from render:
//!
//! ```rust,ignore
//! let level = use_chrome_level(3);
//! level.crumbs(vec![Crumb::here("Keys A").with_menu(sibling_lanes)]);
//! level.tabs(vec![ChromeTab::new("layer", "Layer", page() == Page::Layer, pick)]);
//! level.panels(vec![PanelSpec::new("browser", "Soundsources", Icon::Browser)]);
//! ```
//!
//! Publishing is guarded by equality, so a view can publish every render
//! without looping, and a level clears itself on unmount — leaving the layer
//! zoom takes its crumbs, tabs and panels with it and leaves the rig's alone.
//! Panel *content* stays with the view that owns it: the rail only says which
//! panel is open, and the view renders it with [`PanelHost`].

use std::collections::BTreeMap;

use dioxus::prelude::*;

mod frame;
mod icon;

pub use frame::{
    AppFrame, Crumbs, DragSpace, IconRail, PanelHost, PanelRail, TopBar, TrafficLights,
    WindowButton, WindowCluster,
};
pub use icon::{Glyph, Icon};

/// A step in the crumb trail. Clicking it navigates there; if it carries
/// `menu` items, clicking opens the list of its siblings instead — which is
/// how the chrome replaces every `‹ Back` button in the app.
#[derive(Clone)]
pub struct Crumb {
    pub label: String,
    /// Go to this level.
    pub on_click: Option<Callback<()>>,
    /// Sibling picker: `(label, is_current, select)`.
    pub menu: Vec<(String, bool, Callback<()>)>,
}

impl Crumb {
    /// A plain crumb — a label with somewhere to go.
    pub fn new(label: impl Into<String>, on_click: Callback<()>) -> Self {
        Self {
            label: label.into(),
            on_click: Some(on_click),
            menu: Vec::new(),
        }
    }

    /// The last crumb: where you are, so nothing to click.
    pub fn here(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            on_click: None,
            menu: Vec::new(),
        }
    }

    /// A crumb that opens a picker of its siblings.
    pub fn with_menu(mut self, menu: Vec<(String, bool, Callback<()>)>) -> Self {
        self.menu = menu;
        self
    }
}

impl PartialEq for Crumb {
    /// Data only — callbacks are freshly built every render and would make
    /// every publish look like a change.
    fn eq(&self, other: &Self) -> bool {
        self.label == other.label
            && self.on_click.is_some() == other.on_click.is_some()
            && self.menu.len() == other.menu.len()
            && self
                .menu
                .iter()
                .zip(other.menu.iter())
                .all(|(a, b)| a.0 == b.0 && a.1 == b.1)
    }
}

/// A page tab for the current view (Layer · Module · Edit).
#[derive(Clone)]
pub struct ChromeTab {
    pub id: String,
    pub label: String,
    pub active: bool,
    pub on_select: Callback<()>,
}

impl ChromeTab {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        active: bool,
        on_select: Callback<()>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            active,
            on_select,
        }
    }
}

impl PartialEq for ChromeTab {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.label == other.label && self.active == other.active
    }
}

/// A readout in the bar's right cluster. The rig's engine LED, master meter
/// and "4 lanes live" used to be a bar of their own.
#[derive(Clone, PartialEq)]
pub enum StatusItem {
    /// A coloured pill — the active stack lens, the loaded preset.
    Pill {
        text: String,
        fg: String,
        bg: String,
    },
    /// Quiet text.
    Text { text: String },
    /// A state LED, lit or not.
    Dot { on: bool, color: String },
    /// A level meter, 0..1.
    Meter {
        level: f32,
        color: String,
        width_px: u32,
    },
    /// Any other item, wrapped with a `data-testid` so an e2e suite can find
    /// it in the bar. Purely additive — renders exactly like the wrapped item.
    Tagged {
        testid: String,
        item: Box<StatusItem>,
    },
}

impl StatusItem {
    pub fn pill(text: impl Into<String>, fg: impl Into<String>, bg: impl Into<String>) -> Self {
        Self::Pill {
            text: text.into(),
            fg: fg.into(),
            bg: bg.into(),
        }
    }
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }
    pub fn dot(on: bool, color: impl Into<String>) -> Self {
        Self::Dot {
            on,
            color: color.into(),
        }
    }
    pub fn meter(level: f32, color: impl Into<String>) -> Self {
        Self::Meter {
            level,
            color: color.into(),
            width_px: 72,
        }
    }
    /// Wrap an item with a `data-testid` (for e2e suites).
    pub fn tagged(testid: impl Into<String>, item: StatusItem) -> Self {
        Self::Tagged {
            testid: testid.into(),
            item: Box::new(item),
        }
    }
}

/// A destination in the left rail.
#[derive(Clone)]
pub struct RailItem {
    pub id: String,
    pub label: String,
    pub icon: Icon,
    pub active: bool,
    pub on_select: Callback<()>,
}

impl RailItem {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        icon: Icon,
        active: bool,
        on_select: Callback<()>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon,
            active,
            on_select,
        }
    }
}

impl PartialEq for RailItem {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.label == other.label
            && self.icon == other.icon
            && self.active == other.active
    }
}

/// A panel the current view can show in the right rail.
#[derive(Clone, PartialEq)]
pub struct PanelSpec {
    pub id: String,
    pub title: String,
    pub icon: Icon,
    /// Preferred width when open.
    pub width_px: u32,
    /// Optional `data-testid` for the rail's toggle button (e2e suites).
    pub testid: Option<String>,
}

impl PanelSpec {
    pub fn new(id: impl Into<String>, title: impl Into<String>, icon: Icon) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            icon,
            width_px: 320,
            testid: None,
        }
    }

    pub fn width(mut self, px: u32) -> Self {
        self.width_px = px;
        self
    }

    /// Tag the rail's toggle button with a `data-testid`.
    pub fn testid(mut self, id: impl Into<String>) -> Self {
        self.testid = Some(id.into());
        self
    }
}

/// How deep a publisher sits. The app is 0, the workspace 1, the view inside
/// it 2, and so on — every level writes to its own slot, so the rig naming
/// itself never wipes out what the zoom said and leaving a level takes only
/// that level's contributions with it.
pub type Depth = u8;

/// The chrome's shared state. Copy — hold it anywhere, publish from anywhere.
///
/// Each field is keyed by [`Depth`]: the bar concatenates crumbs and panels
/// across levels (`Keys ▸ Keys A ▸ Module B`) and takes the deepest level's
/// tabs and status, because those describe one view, not a path.
#[derive(Clone, Copy)]
pub struct Chrome {
    pub crumbs: Signal<BTreeMap<Depth, Vec<Crumb>>>,
    pub tabs: Signal<BTreeMap<Depth, Vec<ChromeTab>>>,
    pub status: Signal<BTreeMap<Depth, Vec<StatusItem>>>,
    pub panels: Signal<BTreeMap<Depth, Vec<PanelSpec>>>,
    /// Which panel is open.
    pub open_panel: Signal<Option<String>>,
    /// The level beneath the workspace (the rigs, under Signal).
    pub sub_rail: Signal<Vec<RailItem>>,
    /// How many mounted views have claimed the bar ([`use_bar_claim`]). While
    /// any has, the app hides its own [`TopBar`] and the view's header *is*
    /// the one bar — built from the same pieces: [`Crumbs`], [`DragSpace`],
    /// [`WindowCluster`].
    pub bar_claims: Signal<u32>,
}

impl Chrome {
    fn new() -> Self {
        Self {
            crumbs: Signal::new(BTreeMap::new()),
            tabs: Signal::new(BTreeMap::new()),
            status: Signal::new(BTreeMap::new()),
            panels: Signal::new(BTreeMap::new()),
            open_panel: Signal::new(None),
            sub_rail: Signal::new(Vec::new()),
            bar_claims: Signal::new(0),
        }
    }

    /// The trail, root-first.
    pub fn trail(&self) -> Vec<Crumb> {
        self.crumbs.read().values().flatten().cloned().collect()
    }

    /// The deepest level's pages — tabs describe one view, so they replace
    /// rather than stack.
    pub fn active_tabs(&self) -> Vec<ChromeTab> {
        self.tabs
            .read()
            .values()
            .rev()
            .find(|v| !v.is_empty())
            .cloned()
            .unwrap_or_default()
    }

    /// Every level's readouts, shallowest first — the rig's meters and the
    /// zoomed layer's patch describe different things and both belong in the
    /// bar, unlike tabs, which describe one view.
    pub fn active_status(&self) -> Vec<StatusItem> {
        self.status.read().values().flatten().cloned().collect()
    }

    /// Every level's panels, shallowest first — the app's engines/settings sit
    /// under the view's own browser and monitors.
    pub fn all_panels(&self) -> Vec<PanelSpec> {
        self.panels.read().values().flatten().cloned().collect()
    }

    pub fn set_sub_rail(&self, items: Vec<RailItem>) {
        set_if_changed(self.sub_rail, items);
    }

    /// Is this panel the open one?
    pub fn panel_open(&self, id: &str) -> bool {
        self.open_panel.read().as_deref() == Some(id)
    }

    /// Drop everything published at `depth` — what a level does on its way
    /// out, so its crumbs and panels leave with it.
    fn clear(&self, depth: Depth) {
        let mut crumbs = self.crumbs;
        let mut tabs = self.tabs;
        let mut status = self.status;
        let mut panels = self.panels;
        let gone: Vec<String> = panels
            .peek()
            .get(&depth)
            .map(|v| v.iter().map(|p| p.id.clone()).collect())
            .unwrap_or_default();
        if crumbs.peek().contains_key(&depth) {
            crumbs.write().remove(&depth);
        }
        if tabs.peek().contains_key(&depth) {
            tabs.write().remove(&depth);
        }
        if status.peek().contains_key(&depth) {
            status.write().remove(&depth);
        }
        if !gone.is_empty() {
            panels.write().remove(&depth);
            let open = self.open_panel.peek().clone();
            if open.is_some_and(|id| gone.contains(&id)) {
                self.close_panel();
            }
        }
    }

    /// Open it, or close it if it was already open.
    pub fn toggle_panel(&self, id: &str) {
        let mut open = self.open_panel;
        let next = if open.peek().as_deref() == Some(id) {
            None
        } else {
            Some(id.to_string())
        };
        open.set(next);
    }

    pub fn close_panel(&self) {
        let mut open = self.open_panel;
        open.set(None);
    }
}

fn set_if_changed<T: PartialEq + 'static>(mut signal: Signal<T>, next: T) {
    if *signal.peek() != next {
        signal.set(next);
    }
}

/// One level's handle on the chrome. Publish from render — every setter
/// compares before writing, so republishing the same thing is free and no
/// re-render loop is possible. Dropping the level (navigating away) takes its
/// crumbs, tabs, status and panels with it.
#[derive(Clone, Copy)]
pub struct ChromeLevel {
    chrome: Chrome,
    depth: Depth,
}

impl ChromeLevel {
    /// This level's segment of the trail.
    pub fn crumbs(&self, crumbs: Vec<Crumb>) {
        self.put(self.chrome.crumbs, crumbs);
    }

    /// This view's pages. The deepest level's tabs are the ones shown.
    pub fn tabs(&self, tabs: Vec<ChromeTab>) {
        self.put(self.chrome.tabs, tabs);
    }

    /// This view's readouts.
    pub fn status(&self, status: Vec<StatusItem>) {
        self.put(self.chrome.status, status);
    }

    /// The panels this level offers in the right rail.
    pub fn panels(&self, panels: Vec<PanelSpec>) {
        // A panel that goes away closes itself, or leaving a view would
        // strand its flyout open with nothing to render it.
        let open = self.chrome.open_panel.peek().clone();
        if let Some(id) = open {
            let mine = self
                .chrome
                .panels
                .peek()
                .get(&self.depth)
                .cloned()
                .unwrap_or_default();
            let was_mine = mine.iter().any(|p| p.id == id);
            if was_mine && !panels.iter().any(|p| p.id == id) {
                self.chrome.close_panel();
            }
        }
        self.put(self.chrome.panels, panels);
    }

    /// The chrome itself, for opening panels and reading state.
    pub fn chrome(&self) -> Chrome {
        self.chrome
    }

    fn put<T: PartialEq + Clone + 'static>(
        &self,
        mut signal: Signal<BTreeMap<Depth, T>>,
        value: T,
    ) {
        if signal.peek().get(&self.depth) != Some(&value) {
            signal.write().insert(self.depth, value);
        }
    }
}

/// Claim a level of the chrome for this component. Clears on unmount.
pub fn use_chrome_level(depth: Depth) -> ChromeLevel {
    let chrome = use_chrome();
    use_drop(move || chrome.clear(depth));
    ChromeLevel { chrome, depth }
}

impl Chrome {
    /// Whether a view is drawing the bar itself (see [`use_bar_claim`]).
    pub fn bar_claimed(&self) -> bool {
        (self.bar_claims)() > 0
    }
}

/// Claim the app's bar for this view: while it is mounted the app hides its
/// own [`TopBar`], and the view renders one bar of its own that includes
/// [`Crumbs`], a [`DragSpace`] and the [`WindowCluster`]. One bar instead of
/// the app's stacked on top of the view's.
///
/// A no-op without an app around it (standalone, plugin, web remote) — the
/// private fallback chrome has no bar to hide.
pub fn use_bar_claim() {
    let chrome = use_chrome();
    use_hook(move || {
        let mut claims = chrome.bar_claims;
        claims += 1;
    });
    use_drop(move || {
        let mut claims = chrome.bar_claims;
        let n = *claims.peek();
        claims.set(n.saturating_sub(1));
    });
}

/// What the window can do, supplied by the app shell (desktop: drag, zoom,
/// minimise, close; an app settings flyout). Provided with
/// [`provide_window_actions`]; read by [`DragSpace`] and [`WindowCluster`],
/// which render nothing where no shell provided it (web, phone, plugin).
#[derive(Clone, Copy, PartialEq)]
pub struct WindowActions {
    /// Press on empty bar space: move the window.
    pub drag: Callback<()>,
    /// Double-click empty bar space: maximise / restore.
    pub expand: Callback<()>,
    pub minimize: Callback<()>,
    pub maximize: Callback<()>,
    pub close: Callback<()>,
    /// The app's settings flyout, when it has one.
    pub settings: Option<Callback<()>>,
}

/// Install the shell's [`WindowActions`] at the app root.
pub fn provide_window_actions(actions: WindowActions) {
    use_context_provider(|| actions);
}

/// Install the chrome at the app root.
pub fn provide_chrome() -> Chrome {
    use_context_provider(Chrome::new)
}

/// The chrome the enclosing app installed, or a private one when there is no
/// app around it — a rig UI also runs standalone and as a plugin editor, where
/// publishing crumbs should be a no-op rather than a panic. Both branches run
/// unconditionally: a hook behind a fallback would shift hook order.
pub fn use_chrome() -> Chrome {
    let local = use_hook(Chrome::new);
    let shared = use_hook(try_consume_context::<Chrome>);
    shared.unwrap_or(local)
}
