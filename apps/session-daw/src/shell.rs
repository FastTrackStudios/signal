//! The app's frame, shared by the window and the page: the main views, the
//! setlist's own tabs, the transport and the mode, and the Overview's
//! layout.
//!
//! Both hosts draw the same bar over their own panels — the desktop app
//! adds the window's drag surface and the traffic lights' corner, the page
//! adds neither, and everything else here is the same in both. What a host
//! passes in is only what is host-shaped: its transport bar, and the panels
//! a view is made of.
//!
//! Four views, in the order a service goes through them:
//!
//! - **Setup** — the setlist, and the routing. What is done before.
//! - **Performance** — the chart and the progress, for playing from.
//! - **DAW** — the arrangement and the mixer, for working on.
//! - **Overview** — all of it at once: progress across the top, the chart
//!   down the left, the arrangement with the mixer docked under it on the
//!   right. The first of the docked views.

use dioxus::prelude::*;
use session::modes::Mode;

use crate::setlist::Setlist;
#[cfg(feature = "native")]
use crate::versions::Versions;
#[cfg(not(feature = "native"))]
#[component]
fn Versions() -> Element {
    rsx! {}
}

/// How tall the top bar is — room for the traffic lights with air around.
pub const BAR_H: f64 = 40.0;

pub const BAR_BG: &str = "#17181b";
pub const RULE: &str = "#2a2c31";
pub const TEXT: &str = "#e5e7eb";
pub const DIM: &str = "#8b9099";
pub const ACCENT: &str = "#3aa0ff";

/// How much room the top bar has, and so how much of it is spelled out.
///
/// - **Full** — everything, as on a wide screen: the views as a segmented
///   control, tempo and key each labelled, the clock beside the bar.beat.
/// - **Compact** — half a wide screen: the views fold into a menu, tempo
///   and key into one small pill.
/// - **Narrow** — smaller still: the clock and go-to-start/end go too
///   (Home / End on the keyboard, the tabs for the songs).
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum Density {
    Narrow,
    Compact,
    Full,
}

impl Density {
    /// For a bar `width` logical pixels wide.
    #[must_use]
    pub fn for_width(width: f64) -> Self {
        if width >= 1700.0 {
            Self::Full
        } else if width >= 1250.0 {
            Self::Compact
        } else {
            Self::Narrow
        }
    }
}

/// The bar's density, for what sits in it (the transport, the
/// collaboration button); `Full` outside a bar.
#[must_use]
pub fn use_density() -> Density {
    try_use_context::<Signal<Density>>().map_or(Density::Full, |d| d())
}

/// The views the bars switch between.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    Setup,
    Performance,
    /// The arrangement, and the mixer docked under it when it is open.
    Daw,
    Overview,
    /// The chart alone, the whole window.
    Chart,
    /// The song's lyrics, the whole window.
    Lyrics,
    /// The mixer alone, the whole window.
    Mixer,
    /// The selected track's effects as devices, the whole window
    /// (`crate::fx_view`).
    Fx,
    /// The audio and MIDI editor (the expression editor, brought in
    /// next): a place held for it.
    Editor,
}

impl View {
    pub const ALL: [Self; 9] = [
        Self::Performance,
        Self::Overview,
        Self::Chart,
        Self::Lyrics,
        Self::Daw,
        Self::Mixer,
        Self::Fx,
        Self::Editor,
        Self::Setup,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Setup => "Setup",
            Self::Performance => "Performance",
            Self::Daw => "Arrangement",
            Self::Overview => "Overview",
            Self::Chart => "Chart",
            Self::Lyrics => "Lyrics",
            Self::Mixer => "Mixer",
            Self::Fx => "FX",
            Self::Editor => "Editor",
        }
    }

    /// The word under the view's icon in the bottom bar: the name, cut to
    /// a verb where the name is long.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Performance => "Perform",
            Self::Daw => "Arrange",
            other => other.name(),
        }
    }
}

/// The window's size in logical pixels, as the host keeps it: what a view
/// that sizes itself to its panel (the lyrics) starts from before its
/// panel is measured — the window less the bars.
#[derive(Clone, Copy, PartialEq)]
pub struct WindowSize(pub Signal<(f64, f64)>);

/// Back out of the set to where sets are picked (the start screen, and
/// its library), when the host has one: a context the host provides while
/// a set is open. The top bar shows a Back button while it is there.
#[derive(Clone, Copy, PartialEq)]
pub struct Back(pub Callback<()>);

/// The Editor view, until the audio and MIDI editor (the expression
/// editor) is brought in: a place held for it, saying so.
#[component]
pub fn EditorComing() -> Element {
    rsx! {
        div {
            style: "position:absolute; inset:0; display:flex; flex-direction:column; align-items:center; \
                    justify-content:center; gap:10px; color:{DIM}; font-family:system-ui, sans-serif;",
            lucide_dioxus::AudioWaveform { size: 40, color: "currentColor" }
            span { style: "font-size:16px; font-weight:650; color:{TEXT};", "Editor" }
            span {
                style: "font-size:13px; max-width:420px; text-align:center; line-height:1.5;",
                "The audio and MIDI editor comes here: notes, audio, automation, \
                 edited up close for the selected item."
            }
        }
    }
}

/// What stays on screen over every view — the song's progress along the
/// top, the performance transport along the foot — each a setting a
/// switch on the Setup page turns on;
/// a context the shell provides.
#[derive(Clone, Copy, PartialEq)]
pub struct Pins {
    pub progress: Signal<bool>,
    pub transport: Signal<bool>,
    /// Locked: drags scroll, and move nothing (`crate::options::LOCKING`,
    /// which this mirrors so the buttons that show it re-render).
    pub lock: Signal<bool>,
    /// The inspector down the arrangement's right: the selected track's
    /// strip, and its folder's.
    pub inspector: Signal<bool>,
    /// The navigator down the window's left ([`NavigatorColumn`]).
    pub navigator: Signal<bool>,
    /// The browser down the window's left: what to load on the selected
    /// track (`b`, [`crate::browser::BrowserColumn`]).
    pub browser: Signal<bool>,
    /// The Quantize panel over the arrangement's right
    /// (`crate::quantize_panel`).
    pub quantize: Signal<bool>,
}

/// A pane that is either a view of its own or pinned under others — the
/// mixer, the FX. One way in for each: its bottom-bar button, its key,
/// its toolbar switch, all [`Pane::press`].
///
/// Not docked (the default), pressing it is its view, the whole window,
/// and pressing it again is the view it came from. Docked — a right-click
/// on its button, on a screen wide enough — it is pinned under the views
/// it docks in (`hosts`), opened and closed there.
#[derive(Clone, Copy, PartialEq)]
pub struct Pane {
    pub view: Signal<View>,
    /// Its own view.
    pub own: View,
    /// The views it is pinned under, docked; the first is where pressing it
    /// from anywhere else goes.
    pub hosts: &'static [View],
    /// Pinned under the hosts rather than a view of its own.
    pub docked: Signal<bool>,
    /// Whether the pinned pane is showing.
    pub open: Signal<bool>,
    /// The view its own view was opened from, to go back to.
    back: Signal<View>,
}

impl Pane {
    fn new(view: Signal<View>, own: View, hosts: &'static [View]) -> Self {
        Self {
            view,
            own,
            hosts,
            docked: Signal::new(false),
            open: Signal::new(false),
            back: Signal::new(View::Daw),
        }
    }

    /// Open it, or close it.
    pub fn press(self) {
        let (mut view, mut open, mut back) = (self.view, self.open, self.back);
        let now = *view.peek();
        if *self.docked.peek() {
            if self.hosts.contains(&now) {
                open.toggle();
            } else {
                view.set(self.hosts.first().copied().unwrap_or(View::Daw));
                open.set(true);
            }
        } else if now == self.own {
            view.set(*back.peek());
        } else {
            back.set(now);
            view.set(self.own);
        }
    }

    /// Its own view, full size, whether or not it is docked — or, from
    /// there, back to where it was opened from.
    pub fn press_full(self) {
        let (mut view, mut back) = (self.view, self.back);
        let now = *view.peek();
        if now == self.own {
            view.set(*back.peek());
        } else {
            back.set(now);
            view.set(self.own);
        }
    }

    /// Whether it is showing, however it is shown.
    #[must_use]
    pub fn showing(self) -> bool {
        if (self.docked)() {
            self.hosts.contains(&(self.view)()) && (self.open)()
        } else {
            (self.view)() == self.own
        }
    }

    /// Pinned or a view of its own, keeping it showing where it moves to.
    pub fn set_docked(self, on: bool) {
        let (mut docked, mut view, mut open) = (self.docked, self.view, self.open);
        if *docked.peek() == on {
            return;
        }
        let showing = self.showing();
        docked.set(on);
        if showing {
            if on {
                view.set(self.hosts.first().copied().unwrap_or(View::Daw));
                open.set(true);
            } else {
                view.set(self.own);
            }
        }
    }
}

/// The mixer's [`Pane`]: pinned under the arrangement.
#[derive(Clone, Copy, PartialEq)]
pub struct MixerToggle(pub Pane);

impl std::ops::Deref for MixerToggle {
    type Target = Pane;
    fn deref(&self) -> &Pane {
        &self.0
    }
}

/// The FX's [`Pane`]: pinned under the arrangement or the mixer, either.
#[derive(Clone, Copy, PartialEq)]
pub struct FxToggle(pub Pane);

impl std::ops::Deref for FxToggle {
    type Target = Pane;
    fn deref(&self) -> &Pane {
        &self.0
    }
}

/// The Editor's [`Pane`]: pinned under the arrangement — the selected
/// track's notes or audio, up close, under where it sits in the song.
#[derive(Clone, Copy, PartialEq)]
pub struct EditorPane(pub Pane);

impl std::ops::Deref for EditorPane {
    type Target = Pane;
    fn deref(&self) -> &Pane {
        &self.0
    }
}

/// Provide the window's [`MixerToggle`], [`FxToggle`] and [`EditorPane`]
/// over `view`, for a shell.
pub fn use_mixer_toggle(view: Signal<View>) -> MixerToggle {
    // The FX starts docked: on a desktop it is a strip under whatever you
    // are working in, and a view of its own when you ask (right-click).
    use_context_provider(|| {
        let pane = Pane::new(view, View::Fx, &[View::Daw, View::Mixer]);
        let mut docked = pane.docked;
        docked.set(true);
        FxToggle(pane)
    });
    // The Editor starts docked too, under the arrangement: you edit a
    // part where it is in the song.
    use_context_provider(|| {
        let pane = Pane::new(view, View::Editor, &[View::Daw]);
        let mut docked = pane.docked;
        docked.set(true);
        EditorPane(pane)
    });
    use_context_provider(|| MixerToggle(Pane::new(view, View::Mixer, &[View::Daw])))
}

/// How wide a window must be for the mixer to be pinned under the
/// arrangement: a desktop's, not a tablet's.
pub const DOCK_W: f64 = 1200.0;

impl Pins {
    #[must_use]
    pub fn new() -> Self {
        Self {
            // The song's progress over every view from the start.
            progress: Signal::new(true),
            transport: Signal::new(false),
            // Locked where the pointer is a finger, where a drag meant
            // to scroll is the common case; unlocked at a desk, where
            // a mouse drag is meant to edit.
            lock: Signal::new(!cfg!(test) && crate::touch::detected()),
            inspector: Signal::new(true),
            navigator: Signal::new(false),
            browser: Signal::new(false),
            quantize: Signal::new(false),
        }
    }
}

impl Default for Pins {
    fn default() -> Self {
        Self::new()
    }
}

/// Provide the window's [`Pins`], for a shell: and the lock, wherever it
/// is switched (the Setup page, a view's own lock button), is what
/// [`crate::options::LOCKING`] says.
pub fn use_pins() -> Pins {
    let pins = use_context_provider(Pins::new);
    let lock = pins.lock;
    use_effect(move || crate::options::LOCKING.set(lock()));
    pins
}

/// Whether a view carries the progress bar and the transport already
/// (Performance, Overview), or is not about the song (Setup).
const fn carries_its_own(view: View) -> bool {
    matches!(view, View::Performance | View::Overview | View::Setup)
}

/// The song's progress bar over a view, when it is pinned: where the song
/// is and what comes next, whatever else is being looked at — and a
/// section pressed on it plays from there, from any view.
#[component]
pub fn PinnedProgress(view: View) -> Element {
    let pinned = try_use_context::<Pins>().is_some_and(|pins| (pins.progress)());
    if !pinned || carries_its_own(view) {
        return rsx! {};
    }
    rsx! {
        div {
            style: "flex:none; padding:0 10px;",
            crate::progress::ProgressBar { height: "3rem".to_owned() }
        }
    }
}

/// The performance transport under a view, when it is pinned: back a
/// section, play, loop (record, in Record mode), on a section.
#[component]
pub fn PinnedTransport(view: View) -> Element {
    let pinned = try_use_context::<Pins>().is_some_and(|pins| (pins.transport)());
    let record = try_use_context::<Signal<Mode>>().is_some_and(|mode| mode() == Mode::Record);
    if !pinned || carries_its_own(view) {
        return rsx! {};
    }
    rsx! {
        div {
            style: "flex:none; padding:6px 10px 8px;",
            crate::progress::TransportButtons { record }
        }
    }
}

/// The app's outer box, inset from a phone's or a tablet's safe areas
/// (the notch, the home indicator, a landscape screen's rounded sides) and
/// filled round them with the bars' colour: in a page, where
/// `viewport-fit=cover` runs the page under them. Blitz keeps them out of
/// the page already, and there the insets are nothing.
pub const SAFE_AREA: &str = "position:absolute; top:0; left:0; width:100vw; height:100vh; \
    box-sizing:border-box; display:flex; overflow:hidden; background:#17181b; \
    padding:env(safe-area-inset-top, 0px) env(safe-area-inset-right, 0px) \
    env(safe-area-inset-bottom, 0px) env(safe-area-inset-left, 0px);";

/// A press-outside-closes layer under a popup: the whole screen, from
/// wherever the popup is. Past the screen's edges on every side, because
/// Blitz places a `fixed` box from its container rather than the
/// viewport — a layer at 0,0 of a picker in the bottom-right corner
/// covered nothing a finger could press.
pub const SCRIM: &str = "position:fixed; top:-100vh; left:-100vw; width:300vw; height:300vh;";

/// How tall the bottom bar is: an icon with its word under it, for a
/// finger.
pub const BOTTOM_H: f64 = 54.0;

/// How wide a view's button is, with its word under the icon.
const VIEW_W: f64 = 60.0;

/// How wide the modes are (the window's, and the audio's beside it).
const MODES_W: f64 = 130.0;

/// The bottom bar: at the left Setup (and in it, back to the sets), then the
/// views, each an icon with its word under it — Logic's iPad layout, what
/// you look at picked at the bottom. After them, the view's own controls
/// ([`Context`]): the arrangement's inspector, lock and transport; the
/// chart's pages; the mixer's lock and folders. At the right, the modes:
/// what the window is set up for over where the sound comes from.
///
/// On a touchscreen there is no Overview: it is every view at once, which
/// a tablet or a phone has no room for, and each of its parts is a view
/// of its own here.
#[component]
pub fn BottomBar(
    view: Signal<View>,
    mode: Signal<Mode>,
    /// The host's transport, for the arrangement's context — sized for a
    /// finger (`big`).
    #[props(default)]
    transport: Option<Element>,
    /// How wide the bar is, in logical pixels, when the host knows.
    #[props(default)]
    width: Option<f64>,
) -> Element {
    let touch = crate::touch::use_touch();
    let views: Vec<View> = View::ALL
        .into_iter()
        .filter(|v| *v != View::Setup && !(touch && *v == View::Overview))
        .collect();
    // What the view's own controls have, and so how much of the transport
    // is spelled out there: the bar less the Setup button, the
    // views, the arrangement's two switches and the modes.
    #[allow(clippy::cast_precision_loss)]
    let room = width.map_or(f64::INFINITY, |w| {
        w - (3.0 + views.len() as f64) * VIEW_W - 2.0 * VIEW_W - MODES_W - 4.0
    });
    let density = if room >= 480.0 {
        Density::Full
    } else if room >= 370.0 {
        Density::Compact
    } else {
        Density::Narrow
    };
    let mut shared = use_context_provider(|| Signal::new(density));
    use_effect(use_reactive!(|density| {
        if *shared.peek() != density {
            shared.set(density);
        }
    }));
    let mixer = try_use_context::<MixerToggle>().map(|m| m.0);
    let fx = try_use_context::<FxToggle>().map(|f| f.0);
    let editor = try_use_context::<EditorPane>().map(|e| e.0);
    // The pane a view's button is, when it is one.
    let pane = move |each: View| match each {
        View::Mixer => mixer,
        View::Fx => fx,
        View::Editor => editor,
        _ => None,
    };
    // A pane's right-click menu: pinned or a view of its own, on a screen
    // wide enough to pin it.
    let mut menu = use_signal(|| None::<View>);
    let can_dock = !touch && width.is_none_or(|w| w >= DOCK_W);
    let button = move |each: View| {
        let lit = match pane(each) {
            Some(p) => p.showing(),
            None => view() == each,
        };
        rsx! {
            button {
                key: "{each.name()}",
                title: each.name(),
                style: bottom_button(lit, true),
                onclick: move |_| match pane(each) {
                    Some(p) => p.press(),
                    None => view.set(each),
                },
                oncontextmenu: move |e| {
                    if pane(each).is_some() && can_dock {
                        e.prevent_default();
                        menu.set(Some(each));
                    }
                },
                ViewIcon { view: each, color: ink(lit) }
                span { style: LABEL, "{each.label()}" }
            }
        }
    };
    let showing = view();
    rsx! {
        div {
                        style: "position:relative; height:{BOTTOM_H}px; flex:none; display:flex; align-items:stretch; \
                    background:{BAR_BG}; border-top:1px solid {RULE};",
                        // Setup: the set and the song's details, set up before the
            // views are used (and the way back to the sets) — a view of its
            // own, kept apart from the others.
                                    {button(View::Setup)}
            // What stays on over every view, whichever is showing: the
            // performance transport along the foot, the song's progress
            // along the top.
            if let Some(pins) = try_use_context::<Pins>() {
                button {
                    title: "The transport along the foot of every view",
                    style: bottom_button((pins.transport)(), true),
                    onclick: move |_| {
                        let mut on = pins.transport;
                        on.toggle();
                    },
                    lucide_dioxus::CirclePlay { size: 19, color: ink((pins.transport)()) }
                    span { style: LABEL, "Transport" }
                }
                button {
                    title: "The song's progress along the top of every view",
                    style: bottom_button((pins.progress)(), true),
                    onclick: move |_| {
                        let mut on = pins.progress;
                        on.toggle();
                    },
                    lucide_dioxus::PanelTop { size: 19, color: ink((pins.progress)()) }
                    span { style: LABEL, "Progress" }
                }
            }
            Divider {}
            for each in views {
                {button(each)}
            }
            if let Some(m) = menu().and_then(pane) {
                // A press outside closes it.
                div {
                    style: "{SCRIM} z-index:40;",
                    onclick: move |_| menu.set(None),
                }
                div {
                    style: "position:absolute; left:0; bottom:{BOTTOM_H}px; z-index:41; \
                            min-width:200px; padding:6px; background:{BAR_BG}; \
                            border:1px solid {RULE}; border-radius:12px; display:flex; \
                            flex-direction:column; gap:2px; box-shadow:0 10px 30px rgba(0,0,0,0.5);",
                    button {
                        title: "Pin the mixer under the arrangement, rather than a view of its own",
                        style: mode_option((m.docked)()),
                        onclick: move |_| {
                            // It stays showing, in the place it moves to.
                            m.set_docked(!*m.docked.peek());
                            menu.set(None);
                        },
                        "Docked"
                    }
                }
            }
            // The view's own controls, from the views on.
            div {
                                style: "flex:1; min-width:0; display:flex; align-items:stretch; overflow:hidden;",
                Context { view: showing, transport }
            }
            Jobs {}
            PhaseSwitch { mode }
            Modes { mode }
        }
    }
}

/// What is being done off the window's thread (`crate::jobs`): each
/// job's name and how far along, a bar filling under it — nothing at all
/// when nothing is running.
#[component]
fn Jobs() -> Element {
    let mut shown = use_signal(Vec::<(String, usize, usize)>::new);
    use_future(move || async move {
        loop {
            architect::platform::sleep(std::time::Duration::from_millis(200)).await;
            let now = crate::jobs::running();
            if now != *shown.peek() {
                shown.set(now);
            }
        }
    });
    let jobs = shown();
    if jobs.is_empty() {
        return rsx! {};
    }
    rsx! {
        div {
            style: "flex:none; display:flex; align-items:center; gap:10px; padding:0 10px;",
            Divider {}
            for (label, done, total) in jobs {
                div {
                    key: "{label}",
                    title: "{label}: {done} of {total}",
                    style: "display:flex; flex-direction:column; justify-content:center; gap:4px; min-width:110px;",
                    span {
                        style: "font-size:11px; line-height:13px; font-weight:600; color:{TEXT}; white-space:nowrap;",
                        if total > 0 {
                            "{label} {done}/{total}"
                        } else {
                            "{label}…"
                        }
                    }
                    div {
                        style: "height:3px; border-radius:2px; background:{RULE}; overflow:hidden;",
                        div {
                            style: "height:100%; width:{fraction(done, total)}%; background:{TEXT};",
                        }
                    }
                }
            }
        }
    }
}

/// How far along, percent: a job that does not know its length shows a
/// third, so the bar says something is moving.
fn fraction(done: usize, total: usize) -> f64 {
    use num_traits::ToPrimitive as _;
    if total == 0 {
        return 33.0;
    }
    let (done, total) = (
        done.min(total).to_f64().unwrap_or(0.0),
        total.to_f64().unwrap_or(1.0),
    );
    done / total * 100.0
}

/// A word under a bottom-bar icon.
const LABEL: &str = "font-size:10px; line-height:12px; font-weight:600; white-space:nowrap;";

/// A thin rule between the bottom bar's parts.
#[component]
fn Divider() -> Element {
    rsx! {
                div { style: "flex:none; width:1px; margin:12px 0; background:{RULE};" }
    }
}

/// The modes, at the bottom bar's right end, side by side: what the
/// window is set up for (Record, Mix, Live…) and where the sound comes
/// from (Engine, Cue, Remote) — each its value over its word, as the
/// views are, and pressed for its picker.
#[component]
fn Modes(mode: Signal<Mode>) -> Element {
    let mut mode = mode;
    let mut picking = use_signal(|| false);
    rsx! {
        div {
                        style: "position:relative; flex:none; display:flex; align-items:stretch;",
            Divider {}
            button {
                title: "The mode: what the window is set up for",
                style: "{bottom_button(picking(), true)} min-width:76px;",
                onclick: move |_| picking.toggle(),
                span {
                    style: "font-size:13px; line-height:19px; font-weight:700; color:{TEXT}; white-space:nowrap;",
                    "{mode().display_name()}"
                }
                span { style: LABEL, "Mode" }
            }
            AudioBadge { density: Density::Compact, boxed: true }
            if picking() {
                // A press outside closes it.
                div {
                    style: "{SCRIM} z-index:40;",
                    onclick: move |_| picking.set(false),
                }
                div {
                    style: "position:absolute; right:0; bottom:52px; z-index:41; \
                            min-width:180px; padding:6px; background:{BAR_BG}; \
                            border:1px solid {RULE}; border-radius:12px; display:flex; \
                            flex-direction:column; gap:2px; box-shadow:0 10px 30px rgba(0,0,0,0.5);",
                    for each in Mode::ALL {
                        button {
                            style: mode_option(mode() == each),
                            onclick: move |_| {
                                mode.set(each);
                                picking.set(false);
                            },
                            "{each.display_name()}"
                        }
                    }
                }
            }
        }
    }
}

/// The phase the mode is in, left of the mode, for a mode that has
/// phases (Organize: Import, Tempo Map; Mix: Rescue … Overview) —
/// another place to move through them, beside the mixer's rails. The
/// choice is `session::mode_phases`', so a phase picked on the rails
/// shows here, and one picked here moves the rails.
#[component]
fn PhaseSwitch(mode: Signal<Mode>) -> Element {
    let mut picking = use_signal(|| false);
    // Follow a phase chosen elsewhere (the rails are a widget, which
    // cannot set a signal): the choice's version, watched.
    let mut seen = use_signal(session::mode_phases::version);
    use_future(move || async move {
        loop {
            architect::platform::sleep(std::time::Duration::from_millis(150)).await;
            let now = session::mode_phases::version();
            if now != *seen.peek() {
                seen.set(now);
            }
        }
    });
    let _ = seen();
    let phases = session::mode_phases::of(mode());
    let Some(current) = session::mode_phases::current(mode()) else {
        return rsx! {};
    };
    rsx! {
        div {
            style: "position:relative; flex:none; display:flex; align-items:stretch;",
            Divider {}
            button {
                title: "The phase: where in this mode's work you are",
                style: "{bottom_button(picking(), true)} min-width:76px;",
                onclick: move |_| picking.toggle(),
                span {
                    style: "font-size:13px; line-height:19px; font-weight:700; color:{TEXT}; white-space:nowrap;",
                    "{current.name}"
                }
                span { style: LABEL, "Phase" }
            }
            if picking() {
                div {
                    style: "{SCRIM} z-index:40;",
                    onclick: move |_| picking.set(false),
                }
                div {
                    style: "position:absolute; right:0; bottom:52px; z-index:41; \
                            min-width:180px; padding:6px; background:{BAR_BG}; \
                            border:1px solid {RULE}; border-radius:12px; display:flex; \
                            flex-direction:column; gap:2px; box-shadow:0 10px 30px rgba(0,0,0,0.5);",
                    for (n, each) in (1_usize..).zip(phases) {
                        button {
                            style: mode_option(each == current),
                            onclick: move |_| {
                                session::mode_phases::choose(mode(), each.slug);
                                seen.set(session::mode_phases::version());
                                picking.set(false);
                            },
                            "{n}. {each.name}"
                        }
                    }
                }
            }
        }
    }
}

/// A mode in the picker: a row a finger can hit.
pub(crate) fn mode_option(on: bool) -> String {
    let (bg, fg) = if on {
        ("#2a2d33", TEXT)
    } else {
        ("transparent", DIM)
    };
    format!(
        "min-height:44px; padding:0 14px; border:none; border-radius:8px; text-align:left; \
         background:{bg}; color:{fg}; font-family:inherit; font-size:14px; font-weight:600; cursor:pointer;"
    )
}

/// The view's own controls, after the views in the bottom bar: the
/// arrangement's inspector, lock and transport (where it is, its tempo
/// and key); the chart's pages; the mixer's lock and folders.
#[component]
fn Context(view: View, transport: Option<Element>) -> Element {
    let pins = try_use_context::<Pins>();
    match view {
        View::Daw => rsx! {
            Divider {}
            if let Some(pins) = pins {
                button {
                    title: "The inspector: the selected track's strip, down the arrangement's right",
                    style: bottom_button((pins.inspector)(), true),
                    onclick: move |_| {
                        let mut on = pins.inspector;
                        on.toggle();
                    },
                    lucide_dioxus::PanelRight { size: 19, color: ink((pins.inspector)()) }
                    span { style: LABEL, "Inspect" }
                }
                button {
                    title: "Quantize the drums: a section on its grid, by its groove, previewed first",
                    style: bottom_button((pins.quantize)(), true),
                    onclick: move |_| {
                        let mut on = pins.quantize;
                        on.toggle();
                    },
                    lucide_dioxus::Drum { size: 19, color: ink((pins.quantize)()) }
                    span { style: LABEL, "Quantize" }
                }
                LockButton { lock: pins.lock }
            }
            if let Some(transport) = transport {
                                Divider {}
                div {
                    style: "flex:none; display:flex; align-items:stretch;",
                    {transport}
                }
            }
        },
        View::Chart => rsx! {
            Divider {}
            ChartPages {}
        },
        View::Mixer => rsx! {
            Divider {}
            if let Some(pins) = pins {
                LockButton { lock: pins.lock }
            }
            FolderSwitch {}
        },
        _ => rsx! {},
    }
}

/// The lock, as a button: locked, a drag scrolls and moves nothing.
#[component]
pub(crate) fn LockButton(lock: Signal<bool>) -> Element {
    let mut lock = lock;
    let on = lock();
    rsx! {
        button {
            title: "Locked: a drag scrolls and moves nothing",
            style: bottom_button(on, true),
            onclick: move |_| lock.toggle(),
            if on {
                lucide_dioxus::Lock { size: 19, color: ink(on) }
            } else {
                lucide_dioxus::LockOpen { size: 19, color: ink(on) }
            }
            span { style: LABEL, "Lock" }
        }
    }
}

/// The chart's pages: the one before, back to the one being played, the
/// one after.
#[component]
pub(crate) fn ChartPages() -> Element {
    use crate::chart_panel::{Fit, Turn, set_fit, shown_fit, turn};
    // What the chart shows, and the other way it could: a press switches.
    let mut shown = use_signal(shown_fit);
    let other = match shown() {
        Fit::Page => Fit::Width,
        Fit::Width => Fit::Page,
    };
    let fit_title = match other {
        Fit::Page => "Show the whole page, zoomed out to its full height",
        Fit::Width => "Show the page across the full width, zoomed in, scrolling with the song",
    };
    let fit_name = shown().name();
    rsx! {
        button {
            title: fit_title,
            style: bottom_button(false, true),
            onclick: move |_| {
                set_fit(other);
                shown.set(other);
            },
            if shown() == Fit::Page {
                lucide_dioxus::FileText { size: 19, color: TEXT }
            } else {
                lucide_dioxus::MoveHorizontal { size: 20, color: TEXT }
            }
            span { style: LABEL, "{fit_name}" }
        }
        button {
            title: "The page before",
            style: bottom_button(false, true),
            onclick: move |_| turn(Turn::Back),
            lucide_dioxus::ChevronLeft { size: 22, color: "currentColor" }
            span { style: LABEL, "Page" }
        }
        button {
            title: "Back to the page being played, following the song",
            style: bottom_button(false, true),
            onclick: move |_| turn(Turn::Follow),
            lucide_dioxus::Crosshair { size: 19, color: "currentColor" }
            span { style: LABEL, "Follow" }
        }
        button {
            title: "The page after",
            style: bottom_button(false, true),
            onclick: move |_| turn(Turn::On),
            lucide_dioxus::ChevronRight { size: 22, color: "currentColor" }
            span { style: LABEL, "Page" }
        }
    }
}

/// Every top-level folder folded, or all of them open (`crate::folds`).
#[component]
pub(crate) fn FolderSwitch() -> Element {
    let mut folded = use_signal(crate::folds::tops);
    rsx! {
        button {
            title: if folded() { "Open every folder" } else { "Fold every top-level folder" },
            style: bottom_button(folded(), true),
            onclick: move |_| {
                let on = !folded();
                crate::folds::set_tops(on);
                folded.set(on);
            },
            if folded() {
                lucide_dioxus::FolderClosed { size: 19, color: ink(folded()) }
            } else {
                lucide_dioxus::FolderOpen { size: 19, color: ink(folded()) }
            }
            span { style: LABEL, if folded() { "Folded" } else { "Open" } }
        }
    }
}

/// An icon's colour in a bar: white for what is on or showing, grey for
/// the rest. Given to the icon outright, never as `currentColor` — Blitz
/// resolves an SVG's `currentColor` once, so an icon kept the colour its
/// button had when it was made (the view left stayed white, the view gone
/// to grey).
pub(crate) const fn ink(on: bool) -> &'static str {
    if on { TEXT } else { DIM }
}

/// A bottom-bar button: a grey icon with its word under it, or, for what
/// is showing, white and raised; `label` false, the icon alone.
fn bottom_button(on: bool, label: bool) -> String {
    let (fg, bg) = if on {
        (TEXT, RAISED)
    } else {
        (DIM, "transparent")
    };
    let shape = if label {
        "min-width:58px; padding:0 8px; flex-direction:column; gap:3px;"
    } else {
        "width:52px;"
    };
    // Flat and the bar's full height, as the top bar's parts are: what is
    // showing raised, nothing a card.
    format!(
        "{shape} height:100%; flex:none; box-sizing:border-box; display:flex; align-items:center; \
         justify-content:center; border:none; border-radius:0; background:{bg}; color:{fg}; \
         cursor:pointer; font-family:inherit;"
    )
}

/// A view's icon, as the bottom bar shows it.
#[component]
fn ViewIcon(view: View, color: &'static str) -> Element {
    use lucide_dioxus::{
        ChartNoAxesGantt, FileMusic, LayoutDashboard, ListMusic, MicVocal, Settings,
        SlidersVertical,
    };
    let size = 19;
    match view {
        View::Performance => rsx! { ListMusic { size, color } },
        View::Overview => rsx! { LayoutDashboard { size, color } },
        View::Chart => rsx! { FileMusic { size, color } },
        View::Lyrics => rsx! { MicVocal { size, color } },
        View::Editor => rsx! { lucide_dioxus::AudioWaveform { size, color } },
        View::Daw => rsx! { ChartNoAxesGantt { size, color } },
        View::Mixer => rsx! { SlidersVertical { size, color } },
        View::Fx => rsx! { lucide_dioxus::AudioLines { size, color } },
        View::Setup => rsx! { Settings { size, color } },
    }
}

/// The top bar: the navigator's switch (the whole set down the left, as
/// a vertical progress bar — [`NavigatorColumn`]), the song before, the setlist's tabs, the song
/// after, and the host's badges (who is here) at the right. Back to the
/// sets, the views and the modes are in the bottom bar ([`BottomBar`]).
///
/// `lights` is how much of the left end belongs to the window's own
/// controls (the traffic lights on macOS, nothing in a page), and
/// `on_drag` / `on_zoom` are what a window does with a press on the bar
/// and a double click — a page passes neither.
#[component]
pub fn TopBar(
    /// The host's badges (the collaboration bar).
    badges: Element,
    #[props(default = 12.0)] lights: f64,
    on_drag: Option<EventHandler<()>>,
    on_zoom: Option<EventHandler<()>>,
    /// Picking a song from the setlist tabs.
    on_pick: Option<EventHandler<usize>>,
    /// Recolouring a song from its tab (see [`SongTabs`]).
    on_color: Option<EventHandler<(usize, Option<String>)>>,
    /// How wide the bar is, in logical pixels, when the host knows (a
    /// window does; `None` lays it out in full).
    #[props(default)]
    width: Option<f64>,
) -> Element {
    let density = width.map_or(Density::Full, Density::for_width);
    let mut shared = use_context_provider(|| Signal::new(density));
    use_effect(use_reactive!(|density| {
        if *shared.peek() != density {
            shared.set(density);
        }
    }));
    let navigator = try_use_context::<Pins>().map(|pins| pins.navigator);
    let (lit, ink) = if navigator.is_some_and(|open| open()) {
        (RAISED, TEXT)
    } else {
        ("transparent", DIM)
    };
    rsx! {
        div {
            style: "position:relative; height:{BAR_H}px; flex:none; display:flex; \
                    align-items:stretch; padding-left:{lights}px; background:{BAR_BG}; \
                    border-bottom:1px solid {RULE}; z-index:30;",
            onmousedown: move |_| {
                if let Some(drag) = on_drag {
                    drag.call(());
                }
            },
            ondoubleclick: move |_| {
                if let Some(zoom) = on_zoom {
                    zoom.call(());
                }
            },
            button {
                title: "The navigator: every song in the set, and the sections of the one playing",
                style: "flex:none; width:48px; display:flex; align-items:center; \
                        justify-content:center; padding:0; border:none; \
                        background:{lit}; color:{ink}; cursor:pointer;",
                onmousedown: move |event| event.stop_propagation(),
                onclick: move |_| {
                    if let Some(mut open) = navigator {
                        open.toggle();
                    }
                },
                lucide_dioxus::Menu { size: 18, color: ink }
            }
            Rule {}
            SongTabs { on_pick, on_color, max_shown: tabs_shown(width, crate::touch::use_touch()) }
            Rule {}
            // Save, and the song's checkpoints.
            if cfg!(feature = "native") {
                Versions {}
                Rule {}
            }
            // Whatever the host puts here (who is here, together).
            div {
                style: "flex:none; display:flex; align-items:stretch;",
                onmousedown: move |event| event.stop_propagation(),
                {badges}
            }
        }
    }
}

/// The navigator, down the window's left beside the views, which move
/// over for it: the set as a vertical progress bar
/// ([`crate::navigator::Navigator`]) — every song filled as far as the set
/// has got through it, the one playing opened into its sections. A song
/// pressed is gone to; a section of the one playing, played from. Opened
/// and closed from the top bar's corner, and it stays as it is left.
#[component]
pub fn NavigatorColumn(on_pick: EventHandler<usize>) -> Element {
    // Both read every time: `try_use_context` is a hook, and one skipped
    // on some renders (short-circuited behind `||`) panics on the next.
    let open = try_use_context::<Pins>().is_some_and(|pins| (pins.navigator)());
    let setlist = try_use_context::<Signal<Setlist>>();
    if !open || setlist.is_none() {
        return rsx! {};
    }
    rsx! {
        div {
            style: "position:relative; flex:none; width:300px; background:{BAR_BG}; \
                    border-right:1px solid {RULE};",
            crate::navigator::Navigator { on_pick }
        }
    }
}

/// The top bar's separator: a hairline between its parts, which are flat
/// and the bar's full height rather than cards on it.
#[component]
fn Rule() -> Element {
    rsx! {
        div { style: "flex:none; width:1px; margin:9px 0; background:{RULE};" }
    }
}

/// What a raised part of a bar is: the song showing, a panel open.
pub const RAISED: &str = "#26292f";

/// How many song tabs the top bar shows at once: on a touchscreen, five
/// across a tablet on its side, three held upright, the current and next
/// on a phone; with a mouse, as many as fit at a readable width.
fn tabs_shown(width: Option<f64>, touch: bool) -> usize {
    /// The narrowest a tab is and still names its song.
    const TAB_MIN: f64 = 110.0;
    /// What the bar's other controls take.
    const CONTROLS: f64 = 560.0;
    let Some(width) = width else { return 0 };
    if width < 700.0 {
        2
    } else if touch {
        if width >= 1000.0 { 5 } else { 3 }
    } else {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a tab count"
        )]
        let fit = ((width - CONTROLS) / TAB_MIN).floor().max(3.0) as usize;
        fit
    }
}

/// The audio mode, in the bar: a dot in the colour of what is possible now
/// (Engine green, Cue amber, Remote blue) and as many words as the bar has
/// room for — `Remote · REAPER`, `Engine · loading 12/38` — and, pressed, a
/// menu that says what was asked for, what is driven, how far loading has
/// got, and offers the three modes (see [`crate::audio_mode`]).
///
/// `boxed`, it is the bottom bar's indicator beside the mode ([`Modes`]):
/// an icon in the mode's colour, its menu opening upward.
#[component]
pub fn AudioBadge(density: Density, #[props(default)] boxed: bool) -> Element {
    let mut open = use_signal(|| false);
    // A note under the picker: a mode that needs another launch.
    let mut note = use_signal(|| None::<String>);
    // The mode changes off the UI thread (a loader, the attach), so it is
    // polled — cheaply, by revision.
    let revision = use_signal(crate::audio_mode::revision);
    #[cfg(feature = "native")]
    use_future(move || async move {
        let mut revision = revision;
        loop {
            futures_timer::Delay::new(std::time::Duration::from_millis(250)).await;
            let now = crate::audio_mode::revision();
            if *revision.peek() != now {
                revision.set(now);
            }
        }
    });
    let _ = revision();
    let state = crate::audio_mode::state();
    let effective = state.effective();
    let loading =
        state.requested == crate::audio_mode::AudioMode::Engine && !state.assets.complete();
    let label = match density {
        Density::Full => state.label(2),
        Density::Compact if loading => format!(
            "{} {}/{}",
            effective.name(),
            state.assets.loaded,
            state.assets.total
        ),
        Density::Compact => state.label(1),
        Density::Narrow if loading => format!("{}/{}", state.assets.loaded, state.assets.total),
        Density::Narrow => String::new(),
    };
    let dot = effective.color();
    let target = state
        .target
        .as_ref()
        .map(crate::audio_mode::RemoteTarget::describe);
    let title = match &target {
        Some(target) => format!("Audio: {} — driving {target}", state.label(2)),
        None => format!("Audio: {}", state.label(2)),
    };
    let (outer, face, menu_at) = if boxed {
        (
            "position:relative; flex:none; display:flex;",
            bottom_button(open(), false),
            "right:0; bottom:52px;",
        )
    } else {
        (
            "position:relative; flex:none;",
            format!(
                "display:flex; align-items:center; gap:6px; height:26px; \
                 padding:0 9px; border-radius:6px; border:1px solid {RULE}; \
                 background:#0f1012; color:{TEXT}; font-size:12px; cursor:pointer; \
                 white-space:nowrap;"
            ),
            "right:0; top:30px;",
        )
    };
    rsx! {
        div {
            style: outer,
            onmousedown: move |event| event.stop_propagation(),
            button {
                title: "{title}",
                style: face,
                onclick: move |_| open.toggle(),
                                                if boxed {
                    // An indicator: the sound, in the colour of where it
                    // comes from (the title and the menu say the rest).
                    lucide_dioxus::AudioLines { size: 20, color: dot }
                } else {
                    span { style: "flex:none; width:8px; height:8px; border-radius:4px; background:{dot};" }
                    if !label.is_empty() {
                        span { style: "font-weight:600; overflow:hidden; text-overflow:ellipsis;", "{label}" }
                    }
                }
            }
            if open() {
                if boxed {
                    // A press outside closes it.
                    div {
                        style: "{SCRIM} z-index:40;",
                        onclick: move |_| open.set(false),
                    }
                }
                div {
                    style: "position:absolute; {menu_at} z-index:41; \
                            width:260px; padding:6px; background:{BAR_BG}; \
                            border:1px solid {RULE}; border-radius:8px; \
                            box-shadow:0 8px 24px rgba(0,0,0,0.5); font-size:12px;",
                    div {
                        style: "padding:4px 10px 6px; color:{DIM}; display:flex; flex-direction:column; gap:3px;",
                        div {
                            "Now "
                            span { style: "color:{TEXT}; font-weight:600;", "{effective.name()}" }
                            if effective != state.requested {
                                " — asked for {state.requested.name()}"
                            }
                        }
                        if let Some(target) = target.clone() {
                            div { "Driving " span { style: "color:{TEXT};", "{target}" } }
                        }
                        if loading {
                            div { "Loading media {state.assets.loaded} of {state.assets.total} — silent until each lands" }
                        }
                        if state.requested == crate::audio_mode::AudioMode::Cue && !state.cue_ready {
                            div { "No cue engine yet: Cue plays as Remote" }
                        }
                    }
                    div { style: "height:1px; margin:2px 4px 4px; background:{RULE};" }
                    for each in crate::audio_mode::AudioMode::ALL {
                        div {
                            style: option(state.requested == each),
                            onclick: move |_| {
                                let applied = pick_audio_mode(each);
                                note.set((!applied).then(|| format!("{} applies on the next launch", each.name())));
                            },
                            div { style: "font-weight:600;", "{each.name()}" }
                            div { style: "font-size:11px; color:{DIM};", "{each.blurb()}" }
                        }
                    }
                    if let Some(text) = note() {
                        div { style: "padding:6px 10px 2px; color:#e3b341; font-size:11px;", "{text}" }
                    }
                }
            }
        }
    }
}

/// Ask for `mode`: at once when it drives the same backend (Remote ⇄ Cue),
/// else remembered for the next launch (`false`).
#[cfg(feature = "native")]
pub(crate) fn pick_audio_mode(mode: crate::audio_mode::AudioMode) -> bool {
    crate::open::request_mode(mode)
}

/// A page cannot change what it is: it is shown, not picked.
#[cfg(not(feature = "native"))]
pub(crate) fn pick_audio_mode(mode: crate::audio_mode::AudioMode) -> bool {
    crate::audio_mode::state().requested == mode
}

/// A small down chevron, for a button that opens a menu.
#[component]
pub fn Chevron() -> Element {
    rsx! {
        svg {
            width: "10",
            height: "10",
            view_box: "0 0 10 10",
            fill: "none",
            path { d: "M2 3.5 L5 6.5 L8 3.5", stroke: DIM, stroke_width: "1.5", stroke_linecap: "round", stroke_linejoin: "round" }
        }
    }
}

/// Who else in the session is on a song, for its tab.
#[cfg(feature = "native")]
fn peer_dots(project: String) -> Element {
    rsx! { crate::collab_bar::PeerDots { project } }
}

#[cfg(not(feature = "native"))]
fn peer_dots(_project: String) -> Element {
    rsx! {}
}

/// The setlist across the bar, as Safari lays out its tabs: one rounded
/// strip in the middle of the bar, a tab per song sharing its width, the
/// current one raised.
///
/// Each tab carries its song's colour — a dot beside the title, and a
/// progress line along its foot filled to how far through the song the
/// set is (the playhead in the current song, where it was left in the
/// others). The colour is the song's own: set by hand from the dot, or
/// derived from its title ([`crate::setlist::title_color`]).
///
/// The tabs SHARE the width rather than each taking a fixed size — three
/// songs are three wide tabs, twelve are twelve narrow ones, and either way
/// the set is one glance rather than a list to read.
#[component]
pub fn SongTabs(
    on_pick: Option<EventHandler<usize>>,
    /// The most tabs shown at once: a longer set shows the ones around the
    /// current song, with the previous and next song a press away either
    /// side and the whole set in the navigator. 0 shows every song.
    #[props(default)]
    max_shown: usize,
    /// A colour picked for a song by hand: its index and the CSS colour,
    /// or `None` for "back to the title's colour".
    on_color: Option<EventHandler<(usize, Option<String>)>>,
) -> Element {
    let Some(setlist) = try_use_context::<Signal<Setlist>>() else {
        // No setlist (a single session opened straight): nothing to show,
        // and the bar's spare width goes to the transport.
        return rsx! { div { style: "flex:1;" } };
    };
    let reading = crate::progress::use_reading();
    let mut coloring = use_signal(|| None::<usize>);
    let list = setlist();
    if list.songs.is_empty() {
        return rsx! { div { style: "flex:1;" } };
    }
    let at = reading().at;
    // The songs still on their way have tabs too, so the row keeps its
    // shape as they arrive.
    let count = list.songs.len() + list.pending.len();
    // Which tabs are shown: every one, or — a set longer than the room —
    // a window round the current song: the one before it, it, and those
    // after, as many as fit.
    let cap = if max_shown == 0 {
        count
    } else {
        max_shown.min(count)
    };
    let start = if cap < count {
        let before = if cap <= 2 { 0 } else { (cap - 1) / 2 };
        list.at.saturating_sub(before).min(count - cap)
    } else {
        0
    };
    let shown = start..start + cap;
    let min_w = cap * 24 + 6;
    let pick = move |index: usize| {
        if let Some(pick) = on_pick {
            pick.call(index);
        }
    };
    let songs_len = list.songs.len();
    let current = list.at;
    let arrow = |enabled: bool| {
        let ink = if enabled { TEXT } else { "#3a3d44" };
        format!(
            "flex:none; width:40px; display:flex; align-items:center; \
             justify-content:center; border:none; background:transparent; \
             color:{ink}; cursor:pointer; padding:0;"
        )
    };
    let tabs = rsx! {
        div {
            // Never narrower than a dot (and who is there) per song: the
            // names give way first, then the bar's other controls.
            style: "position:relative; flex:1; min-width:{min_w}px; display:flex; \
                    align-items:stretch;",
            onmousedown: move |event| event.stop_propagation(),
            for (index, song) in list.songs.iter().cloned().enumerate().filter(|(i, _)| shown.contains(i)) {
                {
                    let current = index == list.at;
                    let percent = list.progress_of(index, at) * 100.0;
                    let ground = if current { RAISED } else { "transparent" };
                    let ink = if current { TEXT } else { DIM };
                    // A hairline between two tabs that are not raised, as
                    // Safari draws it; none beside the current one.
                    let divider = if index > 0 && !current && index != list.at + 1 {
                        RULE
                    } else {
                        "transparent"
                    };
                    let color = song.color.clone();
                    // A set of many songs in a narrow window: tabs too
                    // small for the usual inset keep their dot in view.
                    let pad = if count > 5 { 4 } else { 12 };
                    rsx! {
                        div {
                            key: "{song.project}",
                            style: "position:relative; flex:1; min-width:0; display:flex; \
                                    align-items:center; justify-content:center; gap:7px; \
                                    padding:0 {pad}px; background:{ground}; \
                                    border-left:1px solid {divider}; cursor:default; \
                                    overflow:hidden;",
                            onclick: move |_| {
                                if let Some(pick) = on_pick {
                                    pick.call(index);
                                }
                            },
                            // The song's colour; a click here recolours it —
                            // where a song can be recoloured, else it picks
                            // the tab like the rest of it (a page).
                            div {
                                style: "flex:none; width:9px; height:9px; border-radius:5px; \
                                        background:{color}; cursor:pointer;",
                                onclick: move |event| {
                                    if on_color.is_none() {
                                        return;
                                    }
                                    event.stop_propagation();
                                    coloring.set(if coloring() == Some(index) { None } else { Some(index) });
                                },
                            }
                            span {
                                style: "min-width:0; overflow:hidden; text-overflow:ellipsis; \
                                        white-space:nowrap; color:{ink}; font-size:12px; \
                                        font-weight:600;",
                                "{song.title()}"
                            }
                            {peer_dots(song.project.clone())}
                            // How far through the song: a line along the foot.
                            div {
                                style: "position:absolute; left:0; right:0; bottom:0; \
                                        height:2px; background:#ffffff14;",
                                div {
                                    style: "width:{percent}%; height:2px; background:{color};",
                                }
                            }
                        }
                    }
                }
            }
            // Songs still on their way (a page opens its first song, then
            // the rest behind it): dim, named, a ring pulsing where the
            // colour dot will be.
            if !list.pending.is_empty() {
                style { "@keyframes fts-tab-pulse {{ 0%, 100% {{ opacity: .3 }} 50% {{ opacity: 1 }} }}" }
            }
            for title in list.pending.iter().cloned().enumerate().filter(|(i, _)| shown.contains(&(songs_len + i))).map(|(_, t)| t) {
                div {
                    key: "pending-{title}",
                    title: "{title} — loading",
                    style: "position:relative; flex:1; min-width:0; display:flex; align-items:center; \
                            justify-content:center; gap:7px; padding:0 4px; \
                            opacity:0.5; cursor:default; overflow:hidden;",
                    div {
                        style: "flex:none; width:9px; height:9px; border-radius:5px; box-sizing:border-box; \
                                border:1.5px solid {DIM}; animation:fts-tab-pulse 1.2s ease-in-out infinite;",
                    }
                    span {
                        style: "min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; \
                                color:{DIM}; font-size:12px; font-weight:600;",
                        "{title}"
                    }
                }
            }
            if let Some(index) = coloring() {
                ColorMenu {
                    // Under the tab it belongs to.
                    left: format!(
                        "{:.3}%",
                        (index.saturating_sub(start) as f64 + 0.5) / cap as f64 * 100.0
                    ),
                    on_pick: move |choice: Option<String>| {
                        coloring.set(None);
                        if let Some(color) = on_color {
                            color.call((index, choice));
                        }
                    },
                }
            }
        }
    };
    // The song before and the song after, either side of the tabs; the
    // whole set is the navigator's (see [`TopBar`]).
    rsx! {
        div {
            style: "position:relative; flex:1; min-width:0; display:flex; align-items:stretch;",
            onmousedown: move |event| event.stop_propagation(),
            button {
                title: "The song before",
                style: arrow(current > 0),
                onclick: move |_| {
                    if current > 0 {
                        pick(current - 1);
                    }
                },
                lucide_dioxus::ChevronLeft { size: 16, color: if current > 0 { TEXT } else { "#3a3d44" } }
            }
            {tabs}
            button {
                title: "The song after",
                style: arrow(current + 1 < songs_len),
                onclick: move |_| {
                    if current + 1 < songs_len {
                        pick(current + 1);
                    }
                },
                lucide_dioxus::ChevronRight { size: 16, color: if current + 1 < songs_len { TEXT } else { "#3a3d44" } }
            }
        }
    }
}

/// The colours a song can be given by hand, and "Auto" — back to the
/// colour its title gives it.
#[component]
fn ColorMenu(left: String, on_pick: EventHandler<Option<String>>) -> Element {
    rsx! {
        div {
            style: "position:absolute; top:{BAR_H}px; left:{left}; margin-left:-86px; z-index:20; \
                    width:172px; padding:8px; display:flex; flex-wrap:wrap; gap:6px; \
                    background:{BAR_BG}; border:1px solid {RULE}; border-radius:9px; \
                    box-shadow:0 8px 24px rgba(0,0,0,0.5);",
            onmousedown: move |event| event.stop_propagation(),
            for color in crate::setlist::COLORS {
                div {
                    style: "width:34px; height:20px; border-radius:5px; background:{color}; \
                            cursor:pointer;",
                    onclick: move |_| on_pick.call(Some(color.to_owned())),
                }
            }
            div {
                style: "flex:1; min-width:100%; height:22px; display:flex; align-items:center; \
                        justify-content:center; border-radius:5px; border:1px solid {RULE}; \
                        color:{DIM}; font-size:11px; cursor:pointer;",
                onclick: move |_| on_pick.call(None),
                "Auto — from the title"
            }
        }
    }
}

/// The Overview: the progress across the top, the chart down the left, and
/// the arrangement (with the mixer docked under it) on the right.
///
/// One page that answers the three questions a service asks at once —
/// where are we in the song, what is coming, and what is playing — which
/// is why it is the first of the docked views rather than a fourth panel
/// in a stack.
///
/// `editor`, when a host passes one, is the chart as text: a column left of
/// the chart, which then gives up some of its width. `chart_corner` sits
/// over the chart's top-left corner — the desktop's button that opens and
/// closes that editor.
#[component]
pub fn OverviewLayout(
    progress: Element,
    chart: Element,
    panels: Element,
    #[props(default)] editor: Option<Element>,
    #[props(default)] chart_corner: Option<Element>,
    #[props(default)] under_chart: Option<Element>,
) -> Element {
    // The chart takes the column's full width, and — with something under
    // it — the larger share of its height; the rest goes under it.
    let chart_h = if under_chart.is_some() {
        format!(
            "aspect-ratio:{};",
            crate::chart_panel::FITTED_WIDTH_OVER_HEIGHT
        )
    } else {
        "height:100%;".to_owned()
    };
    let chart_w = if editor.is_some() {
        "width:30%; min-width:240px;"
    } else {
        "width:38%; min-width:280px;"
    };
    rsx! {
        div {
            style: "position:absolute; top:0; left:0; right:0; bottom:0; display:flex; \
                    flex-direction:column; gap:10px; padding:10px;",
            div { style: "flex:none;", onmounted: move |e| region("progress", &e), {progress} }
            div {
                style: "flex:1; min-height:0; display:flex; gap:10px;",
                if let Some(editor) = editor {
                    // A chart's lines are short: the editor needs a column,
                    // not a share of the window.
                    // Every column is `height:100%`, not the row's stretch:
                    // what is inside is placed absolutely, and Blitz laid it
                    // out against the column's height BEFORE the stretch —
                    // 2px, the editor a one-line sliver — until something
                    // (a click) laid the pane out again.
                    div {
                        style: "position:relative; flex:none; width:380px; height:100%; \
                                border-radius:8px; overflow:hidden; border:1px solid {RULE};",
                        onmounted: move |e| region("editor", &e),
                        {editor}
                    }
                }
                // The chart over whatever the host puts under it — the
                // lyrics, in the room a 16:9 screen leaves below a page —
                // as one pane: the chart exactly its fitted shape across
                // the column's width, the rest below it the lyrics'.
                div {
                    style: "position:relative; {chart_w} height:100%; display:flex; \
                            flex-direction:column; border-radius:8px; overflow:hidden; \
                            border:1px solid {RULE};",
                    div {
                        style: "position:relative; flex:none; width:100%; {chart_h}",
                        onmounted: move |e| region("chart", &e),
                        {chart}
                        if let Some(corner) = chart_corner {
                            div { style: "position:absolute; top:8px; left:8px;", {corner} }
                        }
                    }
                    if let Some(under) = under_chart {
                        div {
                            style: "position:relative; flex:1; min-height:0; width:100%; \
                                    border-top:1px solid {RULE};",
                            onmounted: move |e| region("lyrics", &e),
                            {under}
                        }
                    }
                }
                div {
                    style: "position:relative; flex:1; min-width:0; height:100%; border-radius:8px; \
                            overflow:hidden; border:1px solid {RULE};",
                    onmounted: move |e| region("panels", &e),
                    {panels}
                }
            }
        }
    }
}

/// A pane of the Overview is a place others' pointers can be shown in
/// (see `collab_pointers`).
#[allow(clippy::needless_pass_by_value)]
fn region(id: &str, event: &Event<MountedData>) {
    #[cfg(feature = "native")]
    crate::ghosts::region_mounted(id, event.data());
    #[cfg(not(feature = "native"))]
    let _ = (id, event);
}

/// Whatever it holds, over one song: that song's session, as context — what
/// every panel below it reads.
#[component]
pub fn WithSong(session: crate::studio::StudioSession, children: Element) -> Element {
    use_context_provider(|| session);
    children
}

/// A view button, on or off.
#[must_use]
pub fn segment(on: bool) -> String {
    let (bg, fg) = if on {
        (ACCENT, "#0b0c0e")
    } else {
        ("transparent", DIM)
    };
    format!(
        "height:24px; padding:0 12px; border:none; border-radius:5px; cursor:pointer; \
         background:{bg}; color:{fg}; font-size:12px; font-weight:600;"
    )
}

/// A row in the mode menu.
#[must_use]
pub fn option(on: bool) -> String {
    let (bg, fg) = if on {
        ("#23262c", TEXT)
    } else {
        ("transparent", DIM)
    };
    format!(
        "padding:6px 10px; border-radius:5px; cursor:pointer; background:{bg}; color:{fg}; font-size:12px;"
    )
}
