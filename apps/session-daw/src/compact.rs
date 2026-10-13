//! The app on a small screen — a phone, or a window made that small: the
//! same views and the same components as the wide layout
//! ([`crate::shell`]), rearranged for one hand and a finger.
//!
//! What moves where:
//!
//! - **The top bar** becomes one thin line: the song, and a caret that pulls
//!   down the rest of what the wide bar shows (who is here, the audio mode,
//!   the mode) as a drawer.
//! - **The views** are five, one at a time, picked from a tab bar at the
//!   bottom (a rail down the left in landscape): Control, Chart, Lyrics,
//!   Arrangement, Mixer.
//! - **The transport** sits over the tabs: the song's section bar, slim,
//!   and the performance buttons, compact.
//! - **Control** is the setlist navigator (`session-ui`'s): each song a
//!   bar filled as far as the set has got, the one playing opened into its
//!   sections, each filled as it plays — the vertical progress bar — and
//!   the place another song is picked.
//!
//! A host decides which layout by the window's shape ([`Form::of`]) and
//! passes what is host-shaped: the other four views' panels, and what goes
//! in the drawer.

use dioxus::prelude::*;

use crate::setlist::Setlist;
use crate::shell::{BAR_BG, DIM, RULE, TEXT};

/// The window's shape, and so which layout the app takes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Form {
    /// Room for the wide layout: the top bar and the docked views.
    Wide,
    /// A phone held upright, or a window that narrow.
    Portrait,
    /// A phone on its side, or a window that short.
    Landscape,
}

impl Form {
    /// Narrower than this (and taller than wide) is a phone held upright.
    pub const NARROW: f64 = 700.0;
    /// Shorter than this (and wider than tall) is a phone on its side.
    pub const SHORT: f64 = 500.0;

    /// The form of a window `width` × `height` logical pixels.
    #[must_use]
    pub fn of(width: f64, height: f64) -> Self {
        if width < Self::NARROW && width <= height {
            Self::Portrait
        } else if height < Self::SHORT && width > height {
            Self::Landscape
        } else {
            Self::Wide
        }
    }

    /// Whether this is the small-screen layout.
    #[must_use]
    pub const fn compact(self) -> bool {
        !matches!(self, Self::Wide)
    }
}

/// The window's form, as the host measured it (wide when it did not say).
#[must_use]
pub fn use_form() -> Form {
    try_use_context::<Signal<Form>>().map_or(Form::Wide, |form| form())
}

/// Where a phone on its side has its camera housing, and how far the controls
/// beside it keep clear: `right`, the inspector's strip moving in from that
/// edge; `left`, the views' rail moving to the right edge instead (no gap
/// kept — the view runs under the housing) — and how far the top rows keep
/// clear of the screen's rounded corners (`corner`). Nothing upright, or with no host saying. In
/// logical pixels; the host provides it as a `Signal<Sides>`.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Sides {
    pub left: f64,
    pub right: f64,
    pub corner: f64,
}

/// The sides, as the host measured them (none when it did not say).
#[must_use]
pub fn use_sides() -> Sides {
    try_use_context::<Signal<Sides>>().map_or_else(Sides::default, |sides| sides())
}

/// The views of the small-screen layout: the five in the tab bar, and
/// More — everything else, a page of its own: Setup, the Editor, the
/// modes, the lock.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PhoneView {
    Control,
    Chart,
    Lyrics,
    Arrangement,
    Mixer,
    More,
    /// The setlist, the routing and the settings (`crate::setup`),
    /// reached from More.
    Setup,
    /// The audio and MIDI editor's place, reached from More.
    Editor,
}

impl PhoneView {
    /// The tab bar, in its order.
    pub const ALL: [Self; 6] = [
        Self::Control,
        Self::Chart,
        Self::Lyrics,
        Self::Arrangement,
        Self::Mixer,
        Self::More,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Control => "Perform",
            Self::Chart => "Chart",
            Self::Lyrics => "Lyrics",
            Self::Arrangement => "Arrange",
            Self::Mixer => "Mixer",
            Self::More => "More",
            Self::Setup => "Setup",
            Self::Editor => "Editor",
        }
    }

    /// The tab that is lit for this view: a page reached from More lights
    /// More.
    #[must_use]
    pub const fn tab(self) -> Self {
        match self {
            Self::Setup | Self::Editor => Self::More,
            other => other,
        }
    }

    /// The shell's own pages, drawn by it rather than by the host.
    #[must_use]
    pub const fn own(self) -> bool {
        matches!(self, Self::More | Self::Setup | Self::Editor)
    }
}

/// The height of the song line at the top: a finger's.
const LINE_H: f64 = 44.0;

/// The small-screen frame, over the songs in the setlist (a
/// `Signal<Setlist>` in context, as both hosts keep it).
///
/// `body` is the current view's panel for every view but Control — the host
/// renders it (its chart, its arrangement are host-shaped) — and `drawer`
/// is what the host adds to the pulled-down top (the session bar, the audio
/// mode). `on_pick` is picking a song, as the wide layout's tabs do.
#[component]
pub fn CompactShell(
    form: Form,
    view: Signal<PhoneView>,
    on_pick: EventHandler<usize>,
    /// What the host adds to More: who is here, where the sound comes from.
    more: Element,
    body: Element,
) -> Element {
    let setlist: Signal<Setlist> = use_context();
    let sides = use_sides();
    // The housing on the left: the rail goes to the right edge instead.
    let mirrored = sides.left > 0.0;
    // The record view's song menu (in Control, in record mode) picks as the
    // navigator does.
    use_context_provider(|| crate::record_view::PickSong(on_pick));
    let current = setlist.read().current().cloned();
    // Record mode: Control is the record view (`crate::record_view`),
    // with its own bars and transport.
    let mode = try_use_context::<Signal<session::modes::Mode>>();
    let record = mode.is_some_and(|m| m() == session::modes::Mode::Record);
    let landscape = form == Form::Landscape;
    let direction = if landscape { "row" } else { "column" };
    let panel = rsx! {
        div {
                        // Under the bars (`z-index`): Blitz clips what overflows a
            // panel when it paints but not when it hit-tests, and the
            // navigator's rows scrolled past its edges took the presses
            // meant for the tabs and the transport.
            style: "position:relative; z-index:1; flex:1; min-height:0; min-width:0; overflow:hidden;",
            if view() == PhoneView::Control && record {
                if let Some(song) = &current {
                    crate::shell::WithSong {
                        key: "{song.project}",
                        session: song.session.clone(),
                        crate::record_view::RecordView {}
                    }
                }
                        } else if view() == PhoneView::Control {
                crate::navigator::Navigator { on_pick }
            } else if view() == PhoneView::More {
                                More { view, session: more }
            } else if view() == PhoneView::Setup {
                crate::setup::SetupView { on_pick }
            } else if view() == PhoneView::Editor {
                crate::shell::EditorComing {}
            } else {
                {body}
            }
        }
    };
    // The performance buttons along the foot, over the song playing
    // (upright; on its side they are in the top line). In Control the
    // navigator is the progress, and the buttons are the view's own, full
    // size.
    let control = view() == PhoneView::Control;
    let strip = match &current {
        Some(_) if control && record => rsx! {},
        // More and its pages are not about the song playing.
        _ if view().own() => rsx! {},
        Some(song) => rsx! {
            crate::shell::WithSong {
                key: "{song.project}",
                session: song.session.clone(),
                                div {
                    style: "position:relative; z-index:5; flex:none; display:flex; flex-direction:column; \
                            gap:4px; padding:6px 8px; background:{BAR_BG}; border-top:1px solid {RULE};",
                    crate::progress::TransportButtons {
                        compact: true,
                        height: if control { 60 } else { 44 },
                    }
                }
            }
        },
        None => rsx! {},
    };
    // The song's sections across the top, under the song: where it is
    // and what comes next, as the wide layout pins them — in Perform too,
    // since it stays put whatever the view; not on More's pages.
    let progress = match &current {
        Some(song) if !view().own() => rsx! {
            crate::shell::WithSong {
                key: "{song.project}",
                session: song.session.clone(),
                div {
                                                            // Edge to edge, square: a bar across the screen, not a
                    // card in it.
                    style: "position:relative; z-index:5; flex:none; background:{BAR_BG}; \
                            border-bottom:1px solid {RULE};",
                    // Upright, the sections are too narrow to name.
                    crate::progress::ProgressBar {
                                                height: "1.75rem".to_owned(),
                        labels: landscape,
                        flat: true,
                    }
                }
            }
        },
        _ => rsx! {},
    };
    let controls = use_signal(|| false);
    // On its side a phone has height for little but the view: the song,
    // the transport and the view's own controls share one line across the
    // top, the sections a thin bar under it, and the views are the rail.
    let top_transport = match &current {
        Some(song) if landscape && !view().own() && !(control && record) => rsx! {
            crate::shell::WithSong {
                key: "{song.project}",
                session: song.session.clone(),
                div {
                    style: "flex:none; width:232px; display:flex; flex-direction:column; justify-content:center; \
                            padding:0 4px; border-left:1px solid {RULE};",
                    crate::progress::TransportButtons { compact: true, height: (LINE_H as u32) - 6 }
                }
            }
        },
        _ => rsx! {},
    };
    rsx! {
        // An explicit size, not insets: Blitz gives an absolutely placed box
        // stretched by `top`/`bottom` no height, and the column collapses to
        // its content (the wide shell does the same). In the app the
        // viewport is already the safe area (Blitz's shell keeps the notch
        // and the home indicator out of it); in a page the outer box insets
        // itself from them (`shell::SAFE_AREA`).
        div {
            style: "{crate::shell::SAFE_AREA}",
        div {
            style: "flex:1; min-width:0; min-height:0; display:flex; \
                    flex-direction:{direction}; \
                    background:#0f1012; color:{TEXT}; font-family:system-ui, sans-serif;",
            if landscape {
                div {
                    style: "position:relative; flex:1; min-width:0; min-height:0; display:flex; flex-direction:column;",
                    // The song, the transport and the view's own controls:
                    // one line across the top, edge to edge — the camera
                    // housing is mid-height, so only the screen's rounded
                    // corners are kept clear here.
                    div {
                        style: "position:relative; z-index:5; flex:none; height:{LINE_H}px; display:flex; \
                                align-items:stretch; box-sizing:border-box; padding:0 {sides.corner}px; \
                                background:{BAR_BG}; border-bottom:1px solid {RULE};",
                        div {
                            style: "flex:1; min-width:0; display:flex; flex-direction:column;",
                            SongLine { view, on_pick, bare: true }
                        }
                        {top_transport}
                        if has_controls(view()) {
                            div {
                                style: "flex:none; display:flex; align-items:stretch; border-left:1px solid {RULE};",
                                ViewControls { view: view(), bare: true }
                            }
                        }
                    }
                    {progress}
                    // Under them, the views' rail and the view, the view
                    // running to both edges. The rail is on the left — or,
                    // with the camera housing on the left, on the right, so
                    // no button is under it and no gap is kept for it (the
                    // view runs under the housing instead). With the housing
                    // on the right, the inspector's strip keeps clear of it
                    // itself. Two rows, not one with the rail moved: Blitz
                    // left a box where it was when one was put before it.
                    if mirrored {
                        div {
                            style: "flex:1; min-height:0; display:flex;",
                            div {
                                style: "position:relative; flex:1; min-width:0; display:flex; flex-direction:column;",
                                {panel}
                            }
                            Tabs { view, rail: true, controls, right: true }
                        }
                    } else {
                        div {
                            style: "flex:1; min-height:0; display:flex;",
                            Tabs { view, rail: true, controls }
                            div {
                                style: "position:relative; flex:1; min-width:0; display:flex; flex-direction:column;",
                                {panel}
                            }
                        }
                    }
                }
            } else {
                SongLine { view, on_pick }
                {progress}
                div {
                    style: "position:relative; flex:1; min-height:0; display:flex; flex-direction:column;",
                    {panel}
                    {strip}
                }
                Tabs { view, rail: false, controls }
            }
        }
        }
    }
}

/// The one thin line at the top: the song before, the song — in the
/// middle, and a press on it the set (Perform, the navigator) — and the
/// song after; flat, the line's full height, as the wide bars are.
#[component]
fn SongLine(
    view: Signal<PhoneView>,
    on_pick: EventHandler<usize>,
    /// Inside a line of its own (on its side), without its own rule.
    #[props(default)]
    bare: bool,
) -> Element {
    let mut view = view;
    let setlist: Signal<Setlist> = use_context();
    let list = setlist.read();
    let (name, color) = list.current().map_or((String::new(), DIM.to_owned()), |s| {
        (s.name.clone(), s.color.clone())
    });
    let place = format!("{}/{}", list.at + 1, list.songs.len() + list.pending.len());
    let at = list.at;
    let (before, after) = (at > 0, at + 1 < list.songs.len());
    let arrow = "flex:none; width:48px; display:flex; align-items:center; justify-content:center; \
                 border:none; background:transparent; padding:0; cursor:pointer;";
    let rule = if bare {
        "none".to_owned()
    } else {
        format!("1px solid {RULE}")
    };
    // The navigator's switch, in the corner as the wide top bar has it:
    // the set (Perform, the navigator), and pressed again, back to the
    // view it was pressed from.
    let mut came_from = use_signal(|| PhoneView::Chart);
    let navigating = view() == PhoneView::Control;
    let (lit, ink) = if navigating {
        (crate::shell::RAISED, TEXT)
    } else {
        ("transparent", DIM)
    };
    rsx! {
        div {
                        style: "position:relative; z-index:5; flex:none; height:{LINE_H}px; display:flex; \
                    align-items:stretch; border-bottom:{rule}; background:{BAR_BG};",
                        button {
                title: "The navigator: every song in the set",
                style: "flex:none; width:48px; display:flex; align-items:center; justify-content:center; \
                        border:none; border-right:1px solid {RULE}; background:{lit}; padding:0; cursor:pointer;",
                onclick: move |_| {
                    if navigating {
                        view.set(came_from());
                    } else {
                        came_from.set(view());
                        view.set(PhoneView::Control);
                    }
                },
                lucide_dioxus::Menu { size: 20, color: ink }
            }
            button {
                title: "The song before",
                style: arrow,
                onclick: move |_| {
                    if before {
                        on_pick.call(at - 1);
                    }
                },
                lucide_dioxus::ChevronLeft { size: 20, color: if before { TEXT } else { "#3a3d44" } }
            }
            button {
                title: "The set",
                style: "flex:1; min-width:0; display:flex; align-items:center; justify-content:center; \
                        gap:8px; padding:0 8px; border:none; border-left:1px solid {RULE}; \
                        border-right:1px solid {RULE}; background:transparent; color:{TEXT}; \
                        font-family:inherit; cursor:pointer;",
                onclick: move |_| view.set(PhoneView::Control),
                span { style: "flex:none; width:8px; height:8px; border-radius:4px; background:{color};" }
                span {
                    style: "font-size:14px; font-weight:650; white-space:nowrap; overflow:hidden; \
                            text-overflow:ellipsis; min-width:0;",
                    "{name}"
                }
                span { style: "flex:none; font-size:11px; color:{DIM};", "{place}" }
            }
            button {
                title: "The song after",
                style: arrow,
                onclick: move |_| {
                    if after {
                        on_pick.call(at + 1);
                    }
                },
                lucide_dioxus::ChevronRight { size: 20, color: if after { TEXT } else { "#3a3d44" } }
            }
        }
    }
}

/// The view's own controls, over the transport: the wide bottom bar's for
/// the same view ([`crate::shell::BottomBar`]'s context), a row of them.
#[component]
fn ViewControls(
    view: PhoneView,
    /// Filling the tab bar's other side, rather than a row of its own.
    #[props(default)]
    bare: bool,
) -> Element {
    let pins = try_use_context::<crate::shell::Pins>();
    let row = if bare {
        "flex:1; height:100%; display:flex; align-items:stretch; justify-content:center;"
    } else {
        "flex:none; height:48px; display:flex; align-items:stretch; justify-content:center;"
    };
    match view {
        PhoneView::Chart => rsx! {
            div { style: row, crate::shell::ChartPages {} }
        },
        PhoneView::Mixer => rsx! {
            div {
                style: row,
                if let Some(pins) = pins {
                    crate::shell::LockButton { lock: pins.lock }
                }
                crate::shell::FolderSwitch {}
            }
        },
        PhoneView::Arrangement => rsx! {
            div {
                style: row,
                if let Some(pins) = pins {
                    crate::shell::LockButton { lock: pins.lock }
                }
            }
        },
        _ => rsx! {},
    }
}

/// More: what the tab bar has no room for — Setup (the set, the routing,
/// the settings, and back to the sets), the Editor — who is here and where
/// the sound comes from (the host's `session`), and the window's mode and
/// lock, as rows a thumb can hit.
#[component]
fn More(view: Signal<PhoneView>, session: Element) -> Element {
    let mut view = view;
    let mode = try_use_context::<Signal<session::modes::Mode>>();
    let pins = try_use_context::<crate::shell::Pins>();
    let heading = format!(
        "padding:18px 16px 6px; font-size:11px; font-weight:700; letter-spacing:0.08em; \
         color:{DIM}; border-bottom:1px solid {RULE};"
    );
    let row = format!(
        "width:100%; min-height:52px; display:flex; align-items:center; gap:14px; padding:0 16px; \
         border:none; border-bottom:1px solid {RULE}; background:transparent; color:{TEXT}; \
         font-family:inherit; font-size:15px; font-weight:600; text-align:left; cursor:pointer;"
    );
    rsx! {
        div {
            style: "position:absolute; top:0; left:0; width:100%; height:100%; overflow-y:auto;",
            button {
                style: "{row}",
                onclick: move |_| view.set(PhoneView::Setup),
                lucide_dioxus::Settings { size: 20, color: TEXT }
                span { style: "flex:1;", "Setup" }
                span { style: "font-size:12px; color:{DIM};", "Setlist, routing, settings" }
                lucide_dioxus::ChevronRight { size: 18, color: DIM }
            }
            button {
                style: "{row}",
                onclick: move |_| view.set(PhoneView::Editor),
                lucide_dioxus::AudioWaveform { size: 20, color: TEXT }
                span { style: "flex:1;", "Editor" }
                lucide_dioxus::ChevronRight { size: 18, color: DIM }
            }
            div { style: "{heading}", "SESSION" }
            div {
                style: "display:flex; align-items:center; gap:10px; flex-wrap:wrap; min-height:52px; \
                        padding:8px 16px; border-bottom:1px solid {RULE};",
                {session}
            }
            if let Some(mut mode) = mode {
                div { style: "{heading}", "MODE" }
                for each in session::modes::Mode::ALL {
                    button {
                        key: "{each.display_name()}",
                        style: {
                            let (bg, ink) = if mode() == each { (crate::shell::RAISED, TEXT) } else { ("transparent", DIM) };
                            format!("{row} min-height:48px; background:{bg}; color:{ink};")
                        },
                        onclick: move |_| mode.set(each),
                        span { style: "flex:1;", "{each.display_name()}" }
                        if mode() == each {
                            lucide_dioxus::Check { size: 18, color: TEXT }
                        }
                    }
                }
            }
            if let Some(pins) = pins {
                div { style: "{heading}", "WINDOW" }
                {
                    let mut lock = pins.lock;
                    let on = lock();
                    let (track, knob) = if on { ("#2563eb", "22px") } else { ("#3a3d44", "2px") };
                    rsx! {
                        button {
                            style: "{row}",
                            onclick: move |_| lock.toggle(),
                            span { style: "flex:1;", "Lock" }
                            div {
                                style: "position:relative; flex:none; width:44px; height:24px; border-radius:12px; background:{track};",
                                div { style: "position:absolute; top:2px; left:{knob}; width:20px; height:20px; border-radius:10px; background:#f3f4f6;" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The views, picked: a tab bar along the bottom, or a rail down the left
/// (the right, `right`).
#[component]
fn Tabs(
    view: Signal<PhoneView>,
    rail: bool,
    controls: Signal<bool>,
    #[props(default)] right: bool,
) -> Element {
    let mut controls = controls;
    let buttons = rsx! {
        for each in PhoneView::ALL {
            button {
                key: "{each.name()}",
                style: tab(view().tab() == each, rail),
                onclick: move |_| view.set(each),
                ViewIcon { view: each, color: crate::shell::ink(view().tab() == each) }
                if !rail {
                    span { style: "font-size:10px;", "{each.name()}" }
                }
            }
        }
    };
    if rail {
        let rule = if right { "border-left" } else { "border-right" };
        return rsx! {
            div {
                style: "flex:none; width:60px; display:flex; flex-direction:column; justify-content:center; \
                        background:{BAR_BG}; {rule}:1px solid {RULE};",
                {buttons}
            }
        };
    }
    // Upright: the views on one side, the view's own controls on the other
    // (the chart's pages, the mixer's folders, the arrangement's lock), the
    // bar slid from one to the other by the switch at its end.
    let has = has_controls(view());
    let showing = controls() && has;
    rsx! {
        div {
            style: "position:relative; z-index:5; flex:none; height:56px; display:flex; align-items:stretch; \
                    background:{BAR_BG}; border-top:1px solid {RULE}; overflow:hidden;",
            if has {
                button {
                    title: if showing { "The views" } else { "This view's controls" },
                    style: "flex:none; width:48px; display:flex; align-items:center; justify-content:center; \
                            border:none; border-right:1px solid {RULE}; background:transparent; padding:0; cursor:pointer;",
                    onclick: move |_| controls.toggle(),
                    if showing {
                        lucide_dioxus::LayoutGrid { size: 20, color: TEXT }
                    } else {
                        lucide_dioxus::SlidersHorizontal { size: 20, color: TEXT }
                    }
                }
            }
            // One side at a time, the other not there at all: Blitz
            // hit-tests where a box was laid out before it was moved, so
            // with both sides in a sliding track a press on "Page ›" landed
            // on the tab laid out under it.
            if showing {
                ViewControls { view: view(), bare: true }
            } else {
                div { style: "flex:1; min-width:0; display:flex; align-items:stretch;", {buttons} }
            }
        }
    }
}

/// Whether a view has controls of its own for the bar's other side.
const fn has_controls(view: PhoneView) -> bool {
    matches!(
        view,
        PhoneView::Chart | PhoneView::Mixer | PhoneView::Arrangement
    )
}

/// A tab: flat, as the wide bars are — the one showing raised.
fn tab(on: bool, rail: bool) -> String {
    let fg = crate::shell::ink(on);
    let bg = if on {
        crate::shell::RAISED
    } else {
        "transparent"
    };
    let shape = if rail {
        "height:52px;"
    } else {
        "flex:1; min-width:0; height:100%;"
    };
    format!(
        "{shape} display:flex; flex-direction:column; align-items:center; justify-content:center; \
         gap:3px; border:none; background:{bg}; color:{fg}; font-family:inherit; \
         font-weight:{}; cursor:pointer;",
        if on { 650 } else { 500 }
    )
}

#[component]
/// A tab's icon, in its colour outright (Blitz resolves an SVG's
/// `currentColor` once; see `shell::ink`).
fn ViewIcon(view: PhoneView, color: &'static str) -> Element {
    use lucide_dioxus::{
        AudioWaveform, ChartNoAxesGantt, Ellipsis, FileMusic, ListMusic, MicVocal, Settings,
        SlidersVertical,
    };
    let size = 22;
    match view {
        PhoneView::Control => rsx! { ListMusic { size, color } },
        PhoneView::Chart => rsx! { FileMusic { size, color } },
        PhoneView::Lyrics => rsx! { MicVocal { size, color } },
        PhoneView::Arrangement => rsx! { ChartNoAxesGantt { size, color } },
        PhoneView::Mixer => rsx! { SlidersVertical { size, color } },
        PhoneView::More => rsx! { Ellipsis { size, color } },
        PhoneView::Setup => rsx! { Settings { size, color } },
        PhoneView::Editor => rsx! { AudioWaveform { size, color } },
    }
}

#[cfg(test)]
mod tests {
    use super::Form;

    #[test]
    fn a_phone_is_compact_either_way_up_and_a_desktop_window_is_wide() {
        assert_eq!(Form::of(390.0, 844.0), Form::Portrait);
        assert_eq!(Form::of(844.0, 390.0), Form::Landscape);
        assert_eq!(Form::of(1440.0, 900.0), Form::Wide);
        // An iPad either way up, and a desktop window made narrow but tall.
        assert_eq!(Form::of(820.0, 1180.0), Form::Wide);
        assert_eq!(Form::of(1180.0, 820.0), Form::Wide);
        assert_eq!(Form::of(600.0, 900.0), Form::Portrait);
    }
}
