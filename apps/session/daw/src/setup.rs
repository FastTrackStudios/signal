//! The Setup view: the set, the routing and the settings — what
//! is done before a service rather than during one — and the way back to
//! the sets.
//!
//! Plain DOM, no widget: both hosts show the same page, and what it edits
//! is the [`Setlist`](crate::setlist::Setlist) the window holds. Flat, as
//! the bars are: a header strip with a tab each for the set (a table of
//! hairline rows), the routing and the settings — pages, not cards. The
//! window's switches are the bottom bar's (transport, progress) and the
//! views' own (the lock).

use dioxus::prelude::*;

use crate::setlist::Setlist;
use crate::shell::{DIM, RAISED, RULE, TEXT};

/// Which page of Setup is showing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tab {
    Setlist,
    Routing,
    Settings,
}

impl Tab {
    const ALL: [Self; 3] = [Self::Setlist, Self::Routing, Self::Settings];

    const fn name(self) -> &'static str {
        match self {
            Self::Setlist => "Setlist",
            Self::Routing => "Routing",
            Self::Settings => "Settings",
        }
    }
}

/// The Setup page: a header strip — back to the sets, and a tab each for
/// the setlist, the routing and the settings — over the page picked, each
/// the whole of the view.
#[component]
pub fn SetupView(
    /// Picking a song, when the host can switch to one.
    on_pick: Option<EventHandler<usize>>,
) -> Element {
    let setlist = try_use_context::<Signal<Setlist>>();
    let back = try_use_context::<crate::shell::Back>();
    let mut tab = use_signal(|| Tab::Setlist);
    // A phone: the tabs closer together, the set's summary left to the
    // table.
    let narrow = room() < 520.0;
    let tab_pad = if narrow { 12 } else { 20 };
    let summary = setlist.map(|setlist| {
        let list = setlist();
        let secs: f64 = list
            .songs
            .iter()
            .map(|s| (s.span.1 - s.span.0).max(0.0))
            .sum();
        let songs = list.songs.len() + list.pending.len();
        format!("{songs} songs · {}", length(secs))
    });
    rsx! {
        div {
            style: "position:absolute; top:0; left:0; right:0; bottom:0; display:flex; \
                    flex-direction:column; color:{TEXT}; font-size:13px; background:#0f1012;",
            div {
                style: "flex:none; height:52px; display:flex; align-items:stretch; \
                        border-bottom:1px solid {RULE}; background:#131417;",
                if let Some(crate::shell::Back(back)) = back {
                    button {
                        title: "Back to your sets",
                        style: "flex:none; display:flex; align-items:center; gap:6px; padding:0 16px 0 10px; \
                                border:none; border-right:1px solid {RULE}; background:transparent; \
                                color:{TEXT}; font-family:inherit; font-size:14px; font-weight:600; cursor:pointer;",
                        onclick: move |_| back.call(()),
                        lucide_dioxus::ChevronLeft { size: 20, color: TEXT }
                        "Sets"
                    }
                }
                for each in Tab::ALL {
                    button {
                        key: "{each.name()}",
                        style: {
                            let (bg, ink) = if tab() == each { (RAISED, TEXT) } else { ("transparent", DIM) };
                            format!("flex:none; padding:0 {tab_pad}px; border:none; border-right:1px solid {RULE}; \
                                     background:{bg}; color:{ink}; font-family:inherit; font-size:14px; \
                                     font-weight:650; cursor:pointer;")
                        },
                        onclick: move |_| tab.set(each),
                        "{each.name()}"
                    }
                }
                div { style: "flex:1;" }
                if tab() == Tab::Setlist && !narrow && let Some(summary) = summary {
                    span { style: "align-self:center; padding:0 18px; font-size:13px; color:{DIM}; white-space:nowrap;", "{summary}" }
                }
            }
            div {
                style: "flex:1; min-height:0; overflow-y:auto;",
                match tab() {
                    Tab::Setlist => match setlist {
                        Some(setlist) => rsx! { SongTable { setlist, on_pick } },
                        None => rsx! {
                            div { style: "padding:18px; color:{DIM};", "No setlist — this window opened one session." }
                        },
                    },
                    Tab::Routing => rsx! { Routing {} },
                    Tab::Settings => rsx! { Settings {} },
                }
            }
        }
    }
}

/// The routing: the engine's outputs and what feeds them.
#[component]
fn Routing() -> Element {
    rsx! {
        Heading { title: "Outputs" }
        div {
            style: "padding:14px 18px; max-width:720px; color:{DIM}; line-height:1.6; font-size:14px;",
            "The engine's outputs, and what goes to them: the mains, the in-ear mixes, and which \
             of the session's buses feed each. Not wired yet — the browser plays one stereo \
             output, and the desktop engine's device routing is next."
        }
    }
}

/// The settings: where the sound comes from, and how the screen is
/// touched.
#[component]
fn Settings() -> Element {
    // Re-read after a pick: the mode changes off this page's signals.
    let mut picked = use_signal(|| 0_u32);
    let mut note = use_signal(|| None::<String>);
    let _ = picked();
    let state = crate::audio_mode::state();
    let touch = try_use_context::<crate::touch::Touch>();
    // The device's own audio, where the host has it (an iPhone's, an
    // iPad's): read as the page opens, and again on Refresh or a change.
    let device = try_use_context::<crate::device_audio::DeviceAudio>();
    let mut report = use_signal(move || device.map(|d| d.read.call(())));
    rsx! {
        if let (Some(device), Some(now)) = (device, report()) {
            Heading { title: "This device" }
            Fact { label: "Output", value: if now.output.is_empty() { "None".to_owned() } else { now.output.clone() } }
            Fact { label: "Input", value: if now.input.is_empty() { "None".to_owned() } else { now.input.clone() } }
            Fact {
                label: "Format",
                value: format!("{:.1} kHz · {:.1} ms buffer · {:.1} ms latency", now.sample_rate / 1000.0, now.buffer_ms, now.latency_ms),
            }
            Toggle {
                on: now.speaker,
                label: "Loudspeaker",
                detail: "Play out of this device's own speaker, whatever else is connected.",
                on_change: move |on: bool| {
                    device.change.call(crate::device_audio::Change::Speaker(on));
                    report.set(Some(device.read.call(())));
                },
            }
            Toggle {
                on: now.microphone,
                label: "Microphone",
                detail: "Record as well as play. Off, the session only plays — the best for Bluetooth headphones and speakers. Applies the next time Session opens.",
                on_change: move |on: bool| {
                    device.change.call(crate::device_audio::Change::Microphone(on));
                    report.set(Some(device.read.call(())));
                },
            }
            button {
                style: "align-self:flex-start; margin:10px 18px; height:36px; padding:0 14px; border-radius:18px; \
                        border:1px solid {RULE}; background:transparent; color:{TEXT}; font-family:inherit; font-size:13px; cursor:pointer;",
                onclick: move |_| report.set(Some(device.read.call(()))),
                "Refresh"
            }
        }
        Heading { title: "Audio" }
        for each in crate::audio_mode::AudioMode::ALL {
            {
                let on = state.requested == each;
                let (bg, ink) = if on { (RAISED, TEXT) } else { ("transparent", DIM) };
                rsx! {
                    button {
                        key: "{each.name()}",
                        style: "width:100%; display:flex; align-items:center; gap:14px; padding:12px 18px; \
                                border:none; border-bottom:1px solid {RULE}; background:{bg}; color:{ink}; \
                                text-align:left; font-family:inherit; cursor:pointer;",
                        onclick: move |_| {
                            let applied = crate::shell::pick_audio_mode(each);
                            note.set((!applied).then(|| format!("{} applies on the next launch", each.name())));
                            picked += 1;
                        },
                        span { style: "flex:none; width:10px; height:10px; border-radius:5px; background:{each.color()};" }
                        div {
                            style: "flex:1; min-width:0; display:flex; flex-direction:column; gap:3px;",
                            span { style: "font-size:14px; font-weight:650; color:{TEXT};", "{each.name()}" }
                            span { style: "font-size:12px; color:{DIM};", "{each.blurb()}" }
                        }
                        if on {
                            lucide_dioxus::Check { size: 18, color: TEXT }
                        }
                    }
                }
            }
        }
        if let Some(text) = note() {
            div { style: "padding:10px 18px; color:#e3b341; font-size:12px;", "{text}" }
        }
        if let Some(stems) = listening() {
            Heading { title: "Playback" }
            Toggle {
                on: stems,
                label: "Multitracks",
                detail: stems_detail(stems),
                on_change: move |on: bool| {
                    if on {
                        load_multitracks();
                        picked += 1;
                    }
                },
            }
        }
        if let Some(crate::touch::Touch(on)) = touch {
            Heading { title: "Touch" }
            Switch {
                on,
                label: "Touch mode",
                detail: "Sized for a finger: bigger rows, strips and grips; a drag scrolls.",
            }
        }
    }
}

/// How the streamed songs are heard, where some are: by their stems
/// (`true`) or their reference (`false`). `None` with nothing streamed —
/// and on a page, which offers the stems its own way.
fn listening() -> Option<bool> {
    #[cfg(feature = "native")]
    {
        let stems = crate::reference_play::multitracks();
        (stems || crate::reference_play::any()).then_some(stems)
    }
    #[cfg(not(feature = "native"))]
    {
        None
    }
}

/// What the Multitracks switch says, on or off.
const fn stems_detail(stems: bool) -> &'static str {
    if stems {
        "Every song is heard by its stems, so the mix can be changed."
    } else {
        "Off: each song plays its stereo reference — a few MB — with the guide live over it. On: every stem of every song is downloaded, so the mix can be changed."
    }
}

/// Hear every streamed song by its stems.
fn load_multitracks() {
    #[cfg(feature = "native")]
    crate::reference_play::load_multitracks();
}

/// A fact about the device, as a row: its name and its value.
#[component]
fn Fact(label: &'static str, value: String) -> Element {
    rsx! {
        div {
            style: "display:flex; align-items:center; gap:14px; min-height:48px; padding:0 18px; \
                    border-bottom:1px solid {RULE};",
            span { style: "flex:none; width:90px; font-size:13px; color:{DIM};", "{label}" }
            span { style: "flex:1; min-width:0; font-size:14px; font-weight:600; color:{TEXT};", "{value}" }
        }
    }
}

/// A switch whose state lives elsewhere: shown as `on`, and each press
/// said to `on_change`.
#[component]
fn Toggle(
    on: bool,
    label: &'static str,
    detail: &'static str,
    on_change: EventHandler<bool>,
) -> Element {
    let (track, knob) = if on {
        ("#2563eb", "22px")
    } else {
        ("#3a3d44", "2px")
    };
    rsx! {
        button {
            style: "width:100%; display:flex; align-items:center; gap:14px; padding:12px 18px; \
                    border:none; border-bottom:1px solid {RULE}; background:transparent; color:{TEXT}; \
                    text-align:left; font-family:inherit; cursor:pointer;",
            onclick: move |_| on_change.call(!on),
            div {
                style: "flex:1; min-width:0; display:flex; flex-direction:column; gap:4px;",
                span { style: "font-size:14px; font-weight:650;", "{label}" }
                span { style: "font-size:12px; color:{DIM}; line-height:1.5;", "{detail}" }
            }
            div {
                style: "position:relative; flex:none; width:44px; height:24px; border-radius:12px; background:{track};",
                div { style: "position:absolute; top:2px; left:{knob}; width:20px; height:20px; border-radius:10px; background:#f3f4f6;" }
            }
        }
    }
}

/// A setting that is on or off: its words, and a switch a finger can hit —
/// a row between hairlines.
#[component]
fn Switch(on: Signal<bool>, label: &'static str, detail: &'static str) -> Element {
    let mut signal = on;
    let (track, knob) = if signal() {
        ("#2563eb", "22px")
    } else {
        ("#3a3d44", "2px")
    };
    rsx! {
        button {
            style: "width:100%; display:flex; align-items:center; gap:14px; padding:12px 18px; \
                    border:none; border-bottom:1px solid {RULE}; background:transparent; color:{TEXT}; \
                    text-align:left; font-family:inherit; cursor:pointer;",
            onclick: move |_| signal.toggle(),
            div {
                style: "flex:1; min-width:0; display:flex; flex-direction:column; gap:4px;",
                span { style: "font-size:14px; font-weight:650;", "{label}" }
                span { style: "font-size:12px; color:{DIM}; line-height:1.5;", "{detail}" }
            }
            div {
                style: "position:relative; flex:none; width:44px; height:24px; border-radius:12px; background:{track};",
                div { style: "position:absolute; top:2px; left:{knob}; width:20px; height:20px; border-radius:10px; background:#f3f4f6;" }
            }
        }
    }
}

/// How wide the page is: the window, less the navigator beside it when it
/// is open.
/// Both contexts read every time: `try_use_context` is a hook, never to
/// be called on some renders and not others.
fn room() -> f64 {
    let window = try_use_context::<crate::shell::WindowSize>();
    let navigator = try_use_context::<crate::shell::Pins>().is_some_and(|pins| (pins.navigator)());
    window.map_or(f64::INFINITY, |window| {
        window.0().0 - if navigator { 300.0 } else { 0.0 }
    })
}

/// A length as a person reads it: `4:05`, `1:02:30`.
fn length(secs: f64) -> String {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "whole seconds"
    )]
    let whole = secs.max(0.0).round() as u64;
    let (h, m, s) = (whole / 3600, whole / 60 % 60, whole % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// A section's title in the side column: small capitals over a hairline.
#[component]
fn Heading(title: &'static str) -> Element {
    rsx! {
        div {
            style: "padding:16px 18px 6px; font-size:11px; font-weight:700; letter-spacing:0.08em; \
                    color:{DIM}; border-bottom:1px solid {RULE};",
            "{title.to_uppercase()}"
        }
    }
}

/// The table's columns, as widths: number, song (the rest), key, tempo,
/// length, mode, and the row's own buttons.
const COLUMNS: [&str; 7] = ["44px", "1fr", "72px", "64px", "72px", "96px", "144px"];

/// A cell's style for column `i`.
fn cell(i: usize, align: &str) -> String {
    let width = COLUMNS[i];
    let size = if width == "1fr" {
        "flex:1; min-width:120px;".to_owned()
    } else {
        format!("flex:none; width:{width};")
    };
    format!(
        "{size} display:flex; align-items:center; justify-content:{align}; overflow:hidden; \
         white-space:nowrap;"
    )
}

/// The set, in order, as a table: which is up, what each song is — its
/// key, tempo, length and the mode it is worked in — and moving or taking
/// one out.
#[component]
fn SongTable(setlist: Signal<Setlist>, on_pick: Option<EventHandler<usize>>) -> Element {
    let list = setlist();
    if list.songs.is_empty() && list.pending.is_empty() {
        return rsx! {
            div { style: "padding:18px; color:{DIM};", "No songs loaded yet." }
        };
    }
    let count = list.songs.len();
    // Narrow (a tablet held upright, the navigator open beside it): the
    // length and the mode give way, so the songs keep their names; on a
    // phone the tempo too.
    let room = room();
    let wide = room >= 760.0;
    let roomy = room >= 520.0;
    // A row's buttons: a thumb's width each, a little less on a phone.
    let row_w: u32 = if roomy { 48 } else { 40 };
    let head = "font-size:11px; font-weight:700; letter-spacing:0.08em; color:#6b7280;";
    rsx! {
        div {
            style: "display:flex; flex-direction:column;",
            div {
                style: "display:flex; height:34px; padding:0 8px 0 0; border-bottom:1px solid {RULE}; {head}",
                div { style: cell(0, "center"), "#" }
                div { style: cell(1, "flex-start"), "SONG" }
                div { style: cell(2, "flex-start"), "KEY" }
                                if roomy {
                    div { style: cell(3, "flex-start"), "BPM" }
                }
                                if wide {
                    div { style: cell(4, "flex-start"), "LENGTH" }
                    div { style: cell(5, "flex-start"), "MODE" }
                }
                                div { style: "{cell(6, \"flex-end\")} width:{3 * row_w}px;", "" }
            }
            for (index, song) in list.songs.iter().cloned().enumerate() {
                {
                    let current = index == list.at;
                    let ground = if current { RAISED } else { "transparent" };
                    let ink = if current { TEXT } else { "#c9ccd2" };
                    let project = &song.session.project;
                    let key = crate::arrangement::key_at(project, song.span.0)
                        .map_or_else(|| "—".to_owned(), |k| crate::transport_bar::short_key(&k));
                    let bpm = crate::transport_bar::tempo_text(project.bpm);
                    let long = length(song.span.1 - song.span.0);
                                        let mode = crate::song_modes::of(&song.name)
                        .map_or("—", session::modes::Mode::display_name);
                    let (earlier, later) = (index > 0, index + 1 < count);
                    rsx! {
                        div {
                            key: "{song.project}",
                            style: "position:relative; display:flex; height:56px; padding:0 8px 0 0; \
                                    border-bottom:1px solid {RULE}; background:{ground}; color:{ink};",
                            // The song's colour, down the row's edge.
                            div { style: "position:absolute; left:0; top:0; bottom:0; width:3px; background:{song.color};" }
                            div { style: "{cell(0, \"center\")} color:#6b7280; font-size:13px;", "{index + 1}" }
                            div {
                                style: "{cell(1, \"flex-start\")} gap:10px; cursor:pointer;",
                                onclick: move |_| {
                                    if let Some(pick) = on_pick {
                                        pick.call(index);
                                    }
                                },
                                span {
                                    style: "font-size:15px; font-weight:650; overflow:hidden; text-overflow:ellipsis;",
                                    "{song.title()}"
                                }
                                if current {
                                    span { style: "flex:none; font-size:11px; font-weight:700; color:{song.color};", "NOW" }
                                }
                            }
                            div { style: "{cell(2, \"flex-start\")} font-weight:600;", "{key}" }
                                                        if roomy {
                                div { style: "{cell(3, \"flex-start\")} font-family:ui-monospace, monospace;", "{bpm}" }
                            }
                                                        if wide {
                                div { style: "{cell(4, \"flex-start\")} font-family:ui-monospace, monospace; color:{DIM};", "{long}" }
                                div { style: "{cell(5, \"flex-start\")} color:{DIM};", "{mode}" }
                            }
                            div {
                                                                style: "{cell(6, \"flex-end\")} align-items:stretch; width:{3 * row_w}px;",
                                {row_button("Earlier in the set", earlier, RowIcon::Up,
                                    EventHandler::new(move |()| setlist.write().reorder(index, index - 1)))}
                                {row_button("Later in the set", later, RowIcon::Down,
                                    EventHandler::new(move |()| setlist.write().reorder(index, index + 1)))}
                                {row_button("Take it out of the set", true, RowIcon::Remove,
                                    EventHandler::new(move |()| setlist.write().remove(index)))}
                            }
                        }
                    }
                }
            }
            // Songs still opening: their place in the set, greyed.
            for (i, title) in list.pending.iter().cloned().enumerate() {
                div {
                    key: "pending-{title}",
                    style: "display:flex; height:56px; border-bottom:1px solid {RULE}; color:#6b7280; opacity:0.7;",
                    div { style: cell(0, "center"), "{count + i + 1}" }
                    div { style: cell(1, "flex-start"), "{title} — opening" }
                }
            }
        }
    }
}

/// What a row's button shows.
#[derive(Clone, Copy)]
enum RowIcon {
    Up,
    Down,
    Remove,
}

/// One of a row's buttons: flat, the row's full height, an icon — in its
/// colour outright, not `currentColor`: Blitz resolved an icon's
/// `currentColor` once, so a button made unusable (a row made as the set's
/// last, while it was still arriving) stayed grey once it could be used.
fn row_button(
    title: &'static str,
    enabled: bool,
    icon: RowIcon,
    on_press: EventHandler<()>,
) -> Element {
    let color = if enabled { DIM } else { "#34373d" };
    rsx! {
        button {
            title,
            style: "flex:1; display:flex; align-items:center; justify-content:center; \
                    border:none; background:transparent; cursor:pointer; padding:0;",
            onclick: move |_| {
                if enabled {
                    on_press.call(());
                }
            },
            match icon {
                RowIcon::Up => rsx! { lucide_dioxus::ChevronUp { size: 18, color } },
                RowIcon::Down => rsx! { lucide_dioxus::ChevronDown { size: 18, color } },
                RowIcon::Remove => rsx! { lucide_dioxus::X { size: 16, color } },
            }
        }
    }
}
