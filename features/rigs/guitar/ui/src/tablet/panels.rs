//! The setlist's pickers — the prototype's `PanelView` and its bodies:
//! they slide over the list, inside the sidebar. The sets switcher drops
//! from the title it opened from; the others rise from the bottom, under
//! the thumb. A tap outside closes them.
//!
//!   Sets      Upcoming / Past / No date, each set with its date as a block
//!   Details   event (the ones used, or a new one), date, title — new, edit,
//!             or duplicate for next week
//!   Add       find a song (or make one: key and tempo) and add it
//!   Key, Tempo, Starts on, Profile, Colour — a song's, in this set
//!   Patch     what a section or part plays, where there is no browser

use dioxus::prelude::*;
use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{LibraryModel, PerformanceModel, ProfileEntry};

use super::colors::{add_days, date_label, date_parts, name_colour, next_date_for, set_heading, set_meta, set_name, song_colour, today_iso, when_label, SetMeta, SONG_PALETTE};
use super::marks::ProfileIcon;
use super::setlist::{call, PrimaryButton};
use super::tokens::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DetailsMode {
    New,
    Edit,
    Duplicate,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Panel {
    Sets,
    Add,
    Key(usize),
    Tempo(usize),
    Start(usize),
    Colour(usize),
    Details(DetailsMode),
    /// A song's profile (by index in the set), or the set's default.
    Profile(Option<usize>),
    /// What part `index` of the song up plays.
    Patch(usize),
}

const KEYS: [&str; 12] = ["C", "Db", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B"];

/// Every set's parts, in the set list's order.
fn metas(perf: &PerformanceModel, lib: &LibraryModel) -> Vec<SetMeta> {
    perf.setlists
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let e = lib.setlists.get(i).cloned().unwrap_or_default();
            set_meta(name, &e.event, &e.date, &e.title)
        })
        .collect()
}

#[component]
pub fn PanelView(panel: Panel, perf: PerformanceModel, lib: LibraryModel, on_close: EventHandler<()>) -> Element {
    let mut current = use_signal(|| panel.clone());
    let mut seen = use_signal(|| panel.clone());
    if *seen.peek() != panel {
        seen.set(panel.clone());
        current.set(panel.clone());
    }
    let p = current();
    let set_index = perf.setlist_index as usize;
    let set_name_now = perf.setlists.get(set_index).cloned().unwrap_or_default();
    let entry = lib.setlists.get(set_index).cloned().unwrap_or_default();
    let set_m = set_meta(&set_name_now, &entry.event, &entry.date, &entry.title);
    let song_at = |i: usize| perf.songs.get(i).cloned().unwrap_or_default();
    let (title, sub) = match &p {
        Panel::Sets => ("Sets".to_string(), format!("{} sets", perf.setlists.len())),
        Panel::Details(DetailsMode::New) => ("New set".to_string(), "An event on a date — a title only if the night has one".to_string()),
        Panel::Details(DetailsMode::Edit) => ("Set details".to_string(), set_name_now.clone()),
        Panel::Details(DetailsMode::Duplicate) => ("Duplicate set".to_string(), format!("{} songs from {}", perf.songs.len(), set_heading(&set_m))),
        Panel::Add => ("Add songs".to_string(), format!("To {set_name_now}")),
        Panel::Key(i) => (song_at(*i).name, "Key in this set".to_string()),
        Panel::Tempo(i) => (song_at(*i).name, "Tempo in this set".to_string()),
        Panel::Start(i) => (song_at(*i).name, "The part it starts on".to_string()),
        Panel::Colour(i) => (song_at(*i).name, "Its colour — every set and device shows it".to_string()),
        Panel::Profile(Some(i)) => (song_at(*i).name, String::new()),
        Panel::Profile(None) => ("Default profile".to_string(), String::new()),
        Panel::Patch(k) => {
            let part = perf.parts.get(*k).cloned().unwrap_or_default();
            let song = song_at(perf.song_index as usize).name;
            (format!("{song} · {}", part.name), "The patch it plays".to_string())
        }
    };
    let top = p == Panel::Sets;
    let body = match p.clone() {
        Panel::Sets => rsx! { SetsBody { perf: perf.clone(), lib: lib.clone(), on_close, on_new: move |()| current.set(Panel::Details(DetailsMode::New)) } },
        Panel::Details(mode) => rsx! { DetailsBody { mode, perf: perf.clone(), lib: lib.clone(), on_done: on_close } },
        Panel::Add => rsx! { AddBody { perf: perf.clone(), lib: lib.clone() } },
        Panel::Key(i) => rsx! { KeyBody { index: i, perf: perf.clone(), on_done: on_close } },
        Panel::Tempo(i) => rsx! { TempoBody { index: i, perf: perf.clone() } },
        Panel::Start(i) => rsx! { StartBody { index: i, perf: perf.clone(), lib: lib.clone(), on_done: on_close } },
        Panel::Colour(i) => rsx! { ColourBody { index: i, perf: perf.clone(), on_done: on_close } },
        Panel::Profile(song) => rsx! { ProfileBody { song, perf: perf.clone(), lib: lib.clone(), on_done: on_close } },
        Panel::Patch(k) => rsx! { PatchBody { part: k, perf: perf.clone(), lib: lib.clone(), on_done: on_close } },
    };
    let (justify, radius, edge) = if top {
        ("flex-start", "0 0 14px 14px", format!("border-bottom: 1px solid {RULE_STRONG};"))
    } else {
        ("flex-end", "12px 12px 0 0", "border-top: 1px solid #3a3a42;".to_string())
    };
    rsx! {
        div { style: "position: absolute; left: 0; top: 0; right: 0; bottom: 0; z-index: 20; display: flex; flex-direction: column; justify-content: {justify};",
            button {
                "aria-label": "Close",
                style: "position: absolute; left: 0; top: 0; right: 0; bottom: 0; border: none; background: rgba(0,0,0,0.66); cursor: default;",
                onclick: move |_| on_close.call(()),
            }
            div { style: "position: relative; max-height: {pick(top, 92, 95)}%; display: flex; flex-direction: column; background: #18181c; {edge} border-radius: {radius}; box-shadow: 0 16px 40px rgba(0,0,0,0.55);",
                if !top {
                    span { style: "align-self: center; width: 36px; height: 4px; border-radius: 2px; background: {DIM}; margin-top: 8px;" }
                }
                header { style: "display: flex; align-items: center; gap: 10px; min-height: 52px; padding: 6px 18px 10px; border-bottom: 1px solid {RULE}; box-sizing: border-box;",
                    div { style: "flex: 1; min-width: 0;",
                        div { style: "font-size: 18px; font-weight: 750; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{title}" }
                        if !sub.is_empty() {
                            div { style: "font-size: 13px; color: {INK_3};", "{sub}" }
                        }
                    }
                }
                div { style: "overflow-y: auto; min-height: 0;", {body} }
            }
        }
    }
}

// ── Sets ───────────────────────────────────────────────────────────────────

#[component]
fn SetsBody(perf: PerformanceModel, lib: LibraryModel, on_close: EventHandler<()>, on_new: EventHandler<()>) -> Element {
    let today = today_iso();
    let ms = metas(&perf, &lib);
    let rows: Vec<(usize, SetMeta)> = ms.into_iter().enumerate().collect();
    let mut upcoming: Vec<(usize, SetMeta)> = rows.iter().filter(|(_, m)| !m.date.is_empty() && m.date >= today).cloned().collect();
    upcoming.sort_by(|a, b| a.1.date.cmp(&b.1.date));
    let mut past: Vec<(usize, SetMeta)> = rows.iter().filter(|(_, m)| !m.date.is_empty() && m.date < today).cloned().collect();
    past.sort_by(|a, b| b.1.date.cmp(&a.1.date));
    let undated: Vec<(usize, SetMeta)> = rows.iter().filter(|(_, m)| m.date.is_empty()).cloned().collect();
    rsx! {
        div { style: "padding-bottom: 14px;",
            div { style: "padding: 12px 14px 0; display: flex; flex-direction: column;",
                PrimaryButton { label: "New set", onclick: move |()| on_new.call(()) }
            }
            SetGroup { label: "Upcoming", rows: upcoming, perf: perf.clone(), lib: lib.clone(), on_close }
            SetGroup { label: "Past", rows: past, perf: perf.clone(), lib: lib.clone(), on_close }
            SetGroup { label: "No date", rows: undated, perf: perf.clone(), lib: lib.clone(), on_close }
        }
    }
}

#[component]
fn SetGroup(label: &'static str, rows: Vec<(usize, SetMeta)>, perf: PerformanceModel, lib: LibraryModel, on_close: EventHandler<()>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    if rows.is_empty() {
        return rsx! {};
    }
    let open = perf.setlist_index as usize;
    rsx! {
        div {
            div { style: "padding: 14px 18px 6px; font-size: 11px; font-weight: 700; letter-spacing: 0.1em; text-transform: uppercase; color: {INK_3};", "{label}" }
            for (i, m) in rows {
                {
                    let on = i == open;
                    let parts = date_parts(&m.date);
                    let songs = lib.setlists.get(i).map_or(0, |s| s.songs.len());
                    let when = when_label(&m.date);
                    let rig = rig.clone();
                    rsx! {
                        button {
                            key: "{i}",
                            style: "width: 100%; min-height: 64px; padding: 8px 14px 8px 18px; display: flex; align-items: center; gap: 14px; text-align: left; justify-content: flex-start; border: none; border-top: 1px solid {RULE}; background: {pick(on, UP, CLEAR)}; color: {INK}; font-family: {FONT}; cursor: pointer;",
                            onclick: move |_| {
                                call!(rig, |r| r.select_setlist(i as u32));
                                on_close.call(());
                            },
                            // The date as a block: the day large, month and weekday small.
                            span { style: "width: 44px; flex-shrink: 0; display: flex; flex-direction: column; align-items: center; line-height: 1;",
                                span { style: "font-size: 11px; font-weight: 700; letter-spacing: 0.08em; text-transform: uppercase; color: {INK_3};", "{parts.map_or(\"—\", |p| p.0)}" }
                                span { style: "font-size: 22px; font-weight: 750; margin: 2px 0;", if let Some(p) = parts { "{p.1}" } }
                                span { style: "font-size: 11px; color: {INK_3};", "{parts.map_or(\"\", |p| p.2)}" }
                            }
                            span { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px;",
                                span { style: "font-size: 16px; font-weight: {pick(on, 750, 620)}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{set_heading(&m)}" }
                                span { style: "display: flex; align-items: center; gap: 8px;",
                                    if !m.title.is_empty() && !m.event.is_empty() {
                                        EventChip { event: m.event.clone(), big: false }
                                    }
                                    span { style: "font-size: 13px; color: {INK_3};",
                                        "{songs} songs"
                                        if !when.is_empty() { " · {when}" }
                                    }
                                }
                            }
                            if on {
                                span { style: "flex-shrink: 0; font-size: 11px; font-weight: 700; letter-spacing: 0.1em; text-transform: uppercase; padding: 2px 6px; border-radius: 4px; background: {LIVE_BG}; color: {LIVE};", "Open" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A recurring event, as a chip in its colour.
#[component]
fn EventChip(event: String, big: bool) -> Element {
    let c = name_colour(&event);
    rsx! {
        span { style: "display: inline-flex; align-items: center; gap: 6px; padding: {pick(big, \"5px 10px\", \"2px 8px\")}; border-radius: 999px; background: color-mix(in srgb, {c} 18%, transparent); color: {INK}; font-size: {pick(big, 14, 12)}px; font-weight: 650; white-space: nowrap;",
            span { style: "width: 7px; height: 7px; border-radius: 999px; background: {c};" }
            "{event}"
        }
    }
}

// ── Details ────────────────────────────────────────────────────────────────

#[component]
fn DetailsBody(mode: DetailsMode, perf: PerformanceModel, lib: LibraryModel, on_done: EventHandler<()>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let set_index = perf.setlist_index as usize;
    let all = metas(&perf, &lib);
    let set = all.get(set_index).cloned().unwrap_or_default();
    // Events, most recently used first.
    let mut by_date = all.clone();
    by_date.sort_by(|a, b| b.date.cmp(&a.date));
    let mut events: Vec<String> = Vec::new();
    for m in &by_date {
        if !m.event.is_empty() && !events.iter().any(|e| e.eq_ignore_ascii_case(&m.event)) {
            events.push(m.event.clone());
        }
    }
    let start = match mode {
        DetailsMode::Edit => set.clone(),
        DetailsMode::Duplicate => SetMeta { event: set.event.clone(), date: if set.date.is_empty() { today_iso() } else { add_days(&set.date, 7) }, title: String::new() },
        DetailsMode::New => {
            let ev = if set.event.is_empty() { events.first().cloned().unwrap_or_default() } else { set.event.clone() };
            SetMeta { date: next_date_for(&ev, &all), event: ev, title: String::new() }
        }
    };
    let mut m = use_signal(|| start);
    let mut new_event = use_signal(|| None::<String>);
    let cur = m();
    let name = set_name(&cur);
    let clash = perf.setlists.iter().enumerate().any(|(i, n)| n.eq_ignore_ascii_case(&name) && !(mode == DetailsMode::Edit && i == set_index));
    let ok = !cur.event.trim().is_empty() && !clash;
    let today = today_iso();
    let mut quick: Vec<(String, String)> = vec![("Today".into(), today.clone()), ("Tomorrow".into(), add_days(&today, 1))];
    let others: Vec<SetMeta> = all.iter().enumerate().filter(|(i, _)| mode != DetailsMode::Edit || *i != set_index).map(|(_, x)| x.clone()).collect();
    if !cur.event.is_empty() {
        let usual = next_date_for(&cur.event, &others);
        if usual != today && usual != add_days(&today, 1) {
            quick.push((format!("Next {} · {}", cur.event, date_label(&usual)), usual));
        }
    }
    let field = format!("width: 100%; min-height: 48px; padding: 0 12px; border: 1px solid {RULE_STRONG}; border-radius: {R}; background: #0a0a0d; color: {INK}; font-size: 16px; font-family: {FONT}; box-sizing: border-box;");
    let label = format!("font-size: 11px; font-weight: 700; letter-spacing: 0.1em; text-transform: uppercase; color: {INK_3}; margin-bottom: 8px;");
    let confirm = match mode {
        DetailsMode::Edit => "Save".to_string(),
        DetailsMode::Duplicate => format!("Duplicate {} songs", perf.songs.len()),
        DetailsMode::New => "Create set".to_string(),
    };
    let count = perf.setlists.len() as u32;
    rsx! {
        div { style: "padding: 14px 16px 18px; display: flex; flex-direction: column; gap: 18px;",
            div {
                div { style: "{label}", "Event" }
                div { style: "display: flex; flex-wrap: wrap; gap: 8px;",
                    for e in events.iter().cloned() {
                        {
                            let on = e == cur.event && new_event().is_none();
                            let all = all.clone();
                            rsx! {
                                button {
                                    key: "{e}",
                                    style: "border: none; background: transparent; padding: 0; border-radius: 999px; cursor: pointer; {focus_outline(on)}",
                                    onclick: move |_| {
                                        new_event.set(None);
                                        let mut x = m();
                                        x.event = e.clone();
                                        if mode != DetailsMode::Edit {
                                            x.date = next_date_for(&e, &all);
                                        }
                                        m.set(x);
                                    },
                                    EventChip { event: e.clone(), big: true }
                                }
                            }
                        }
                    }
                    button {
                        style: "min-height: 34px; padding: 0 12px; border-radius: 999px; border: 1px dashed {RULE_STRONG}; background: transparent; color: {INK_2}; font-size: 14px; font-weight: 600; font-family: {FONT}; cursor: pointer;",
                        onclick: move |_| new_event.set(Some(String::new())),
                        "+ New event"
                    }
                }
                if let Some(ne) = new_event() {
                    input {
                        autofocus: true,
                        value: "{ne}",
                        placeholder: "e.g. Sunday AM, Youth, Easter",
                        style: "{field} margin-top: 10px;",
                        oninput: move |e| {
                            let v = e.value();
                            new_event.set(Some(v.clone()));
                            let mut x = m();
                            x.event = v;
                            m.set(x);
                        },
                    }
                }
            }
            div {
                div { style: "{label}", "Date" }
                div { style: "display: flex; flex-wrap: wrap; gap: 8px; margin-bottom: 8px;",
                    for (ql, qd) in quick {
                        {
                            let on = qd == cur.date;
                            rsx! {
                                button {
                                    key: "{ql}",
                                    style: "min-height: 40px; padding: 0 12px; border-radius: {R}; border: {ring(on, 2.0)}; background: {pick(on, UP, CLEAR)}; color: {INK}; font-size: 14px; font-weight: {pick(on, 700, 560)}; font-family: {FONT}; cursor: pointer;",
                                    onclick: move |_| {
                                        let mut x = m();
                                        x.date = qd.clone();
                                        m.set(x);
                                    },
                                    "{ql}"
                                }
                            }
                        }
                    }
                }
                // The date, a day or a week at a time.
                div { style: "display: flex; align-items: center; gap: 6px;",
                    for (lbl, days) in [("−7", -7_i64), ("−1", -1)] {
                        DateStep { key: "{lbl}", label: lbl, days, m }
                    }
                    span { style: "flex: 1; min-height: 48px; display: flex; align-items: center; justify-content: center; border: 1px solid {RULE_STRONG}; border-radius: {R}; font-size: 16px; font-weight: 650;",
                        if cur.date.is_empty() { "No date" } else { "{date_label(&cur.date)}" }
                    }
                    for (lbl, days) in [("+1", 1_i64), ("+7", 7)] {
                        DateStep { key: "{lbl}", label: lbl, days, m }
                    }
                }
            }
            div {
                div { style: "{label}",
                    "Title "
                    span { style: "text-transform: none; letter-spacing: 0; font-weight: 500;", "— optional" }
                }
                input {
                    value: "{cur.title}",
                    placeholder: "Only for a special night — e.g. Worship Night",
                    style: "{field}",
                    oninput: move |e| {
                        let mut x = m();
                        x.title = e.value();
                        m.set(x);
                    },
                }
            }
            div { style: "padding: 12px 14px; border-radius: {R_MD}; background: {SHEET}; border: 1px solid {RULE};",
                div { style: "font-size: 18px; font-weight: 750;", "{set_heading(&cur)}" }
                div { style: "display: flex; align-items: center; gap: 8px; margin-top: 6px;",
                    if !cur.title.is_empty() && !cur.event.is_empty() {
                        EventChip { event: cur.event.clone(), big: false }
                    }
                    span { style: "font-size: 14px; font-weight: 600;", "{date_label(&cur.date)}" }
                    span { style: "font-size: 13px; color: {INK_3};", "{when_label(&cur.date)}" }
                }
                div { style: "margin-top: 8px; font-size: 12px; color: {INK_3};", "Saved as “{name}”" }
                if clash {
                    div { style: "margin-top: 6px; color: {VOID}; font-size: 13px;", "There's already a set for this event on this date." }
                }
            }
            PrimaryButton {
                label: confirm,
                disabled: !ok,
                onclick: move |()| {
                    let x = m();
                    let name = set_name(&x);
                    if let Some(r) = rig.clone() {
                        let _ = dioxus_core::spawn_forever(async move {
                            let at = match mode {
                                DetailsMode::Edit => {
                                    let _ = r.rename_setlist(set_index as u32, name).await;
                                    set_index as u32
                                }
                                DetailsMode::Duplicate => {
                                    let _ = r.duplicate_setlist(set_index as u32, name).await;
                                    count
                                }
                                DetailsMode::New => {
                                    let _ = r.add_setlist(name).await;
                                    count
                                }
                            };
                            let _ = r.set_setlist_details(at, x.event, x.date, x.title).await;
                            if mode != DetailsMode::Edit {
                                let _ = r.select_setlist(at).await;
                            }
                        });
                    }
                    on_done.call(());
                },
            }
        }
    }
}

#[component]
fn DateStep(label: &'static str, days: i64, m: Signal<SetMeta>) -> Element {
    rsx! {
        button {
            style: "width: 52px; min-height: 48px; border-radius: {R}; border: 1px solid {RULE_STRONG}; background: transparent; color: {INK_2}; font-size: 15px; font-weight: 650; font-family: {FONT}; cursor: pointer;",
            onclick: move |_| {
                let mut x = m();
                x.date = add_days(&x.date, days);
                let mut m = m;
                m.set(x);
            },
            "{label}"
        }
    }
}

// ── Add songs ──────────────────────────────────────────────────────────────

#[component]
fn AddBody(perf: PerformanceModel, lib: LibraryModel) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut q = use_signal(String::new);
    let mut making = use_signal(|| false);
    let mut key = use_signal(|| "G".to_string());
    let mut bpm = use_signal(|| 72_u32);
    let set = perf.setlist_index;
    let query = q().trim().to_lowercase();
    let shown: Vec<_> = lib.songs.iter().filter(|s| s.name.to_lowercase().contains(&query)).cloned().collect();
    let exists = lib.songs.iter().any(|s| s.name.to_lowercase() == query);
    let field = format!("width: 100%; min-height: 48px; padding: 0 12px; border: 1px solid {RULE_STRONG}; border-radius: {R}; background: #0a0a0d; color: {INK}; font-size: 16px; font-family: {FONT}; box-sizing: border-box;");
    let typed = q().trim().to_string();
    rsx! {
        div {
            div { style: "padding: 12px 14px; display: flex; flex-direction: column; gap: 8px; border-bottom: 1px solid {RULE};",
                input { value: "{q}", placeholder: "Find a song, or type a new one", style: "{field}", oninput: move |e| q.set(e.value()) }
                if !typed.is_empty() && !exists && !making() {
                    PrimaryButton { label: format!("New song “{typed}”…"), onclick: move |()| making.set(true) }
                }
                if making() {
                    div { style: "display: flex; flex-direction: column; gap: 10px;",
                        KeyGrid { value: key(), on_pick: move |k: String| key.set(k) }
                        div { style: "display: flex; align-items: center; gap: 8px;",
                            span { style: "flex: 1; font-size: 13px; color: {INK_3};", "Tempo" }
                            StepButton { label: "−", onclick: move |()| bpm.set(bpm().saturating_sub(1).max(30)) }
                            span { style: "min-width: 48px; text-align: center; font-size: 18px; font-weight: 700;", "{bpm}" }
                            StepButton { label: "+", onclick: move |()| bpm.set(bpm() + 1) }
                        }
                        div { style: "display: flex; gap: 8px; justify-content: flex-end;",
                            StepButton { label: "Cancel", onclick: move |()| making.set(false) }
                            PrimaryButton {
                                label: "Create and add",
                                onclick: {
                                    let rig = rig.clone();
                                    let typed = typed.clone();
                                    move |()| {
                                        let (name, k, b) = (typed.clone(), key(), bpm());
                                        if let Some(r) = rig.clone() {
                                            let _ = dioxus_core::spawn_forever(async move {
                                                let _ = r.add_song(name.clone(), k, b).await;
                                                let _ = r.add_setlist_entry(set, name).await;
                                            });
                                        }
                                        making.set(false);
                                        q.set(String::new());
                                    }
                                },
                            }
                        }
                    }
                }
            }
            for s in shown {
                {
                    let added = perf.songs.iter().any(|x| x.name.eq_ignore_ascii_case(&s.name));
                    let rig = rig.clone();
                    let name = s.name.clone();
                    rsx! {
                        button {
                            key: "{s.name}",
                            disabled: added,
                            style: "width: 100%; min-height: 56px; padding: 6px 14px 6px 18px; display: flex; align-items: center; gap: 10px; text-align: left; justify-content: flex-start; border: none; border-bottom: 1px solid {RULE}; background: transparent; color: {INK}; font-family: {FONT}; opacity: {pick(added, 0.5, 1.0)}; cursor: pointer;",
                            onclick: move |_| {
                                let name = name.clone();
                                call!(rig, |r| r.add_setlist_entry(set, name));
                            },
                            span { style: "width: 10px; height: 10px; border-radius: 3px; flex-shrink: 0; background: {song_colour(&s.name, &s.colour)};" }
                            span { style: "flex: 1; min-width: 0; font-size: 16px; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{s.name}" }
                            if !s.key.is_empty() {
                                span { style: "min-width: 30px; height: 28px; padding: 0 6px; box-sizing: border-box; border: 1px solid {RULE_STRONG}; border-radius: 4px; display: flex; align-items: center; justify-content: center; font-size: 14px; font-weight: 650; color: {INK_2};", "{s.key}" }
                            }
                            span { style: "width: 30px; text-align: right; color: {INK_2}; font-size: 14px;", if s.bpm > 0 { "{s.bpm}" } }
                            span { style: "width: 46px; text-align: right; font-size: 11px; font-weight: 700; letter-spacing: 0.1em; text-transform: uppercase; color: {pick(added, INK_3, FOCUS_FG)};",
                                if added { "In set" } else { "Add" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn StepButton(label: &'static str, onclick: EventHandler<()>) -> Element {
    rsx! {
        button {
            style: "min-width: 48px; min-height: 48px; padding: 0 12px; border-radius: {R}; border: 1px solid {RULE_STRONG}; background: transparent; color: {INK}; font-size: 17px; font-weight: 650; font-family: {FONT}; cursor: pointer;",
            onclick: move |_| onclick.call(()),
            "{label}"
        }
    }
}

/// The twelve keys and major / minor.
#[component]
fn KeyGrid(value: String, on_pick: EventHandler<String>) -> Element {
    let minor = value.ends_with('m');
    let root = value.trim_end_matches('m').to_string();
    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 8px;",
            div { style: "display: grid; grid-template-columns: repeat(6, minmax(0, 1fr)); gap: 6px;",
                for k in KEYS {
                    {
                        let on = k == root;
                        rsx! {
                            button {
                                key: "{k}",
                                style: "min-height: 48px; border-radius: {R}; border: {ring(on, 2.0)}; background: {pick(on, UP, SHEET)}; color: {INK}; font-size: 16px; font-weight: {pick(on, 750, 560)}; font-family: {FONT}; cursor: pointer;",
                                onclick: move |_| on_pick.call(if minor { format!("{k}m") } else { k.to_string() }),
                                "{k}"
                            }
                        }
                    }
                }
            }
            div { style: "display: flex; padding: 2px; border-radius: {R}; background: {FILL};",
                for (lbl, is_minor) in [("Major", false), ("Minor", true)] {
                    {
                        let on = minor == is_minor;
                        let root = root.clone();
                        rsx! {
                            button {
                                key: "{lbl}",
                                style: "flex: 1; min-height: 40px; border: none; border-radius: 4px; background: {pick(on, FILL_ON, CLEAR)}; color: {pick(on, INK, INK_3)}; font-size: 14px; font-weight: {pick(on, 750, 600)}; font-family: {FONT}; cursor: pointer;",
                                onclick: move |_| on_pick.call(if is_minor { format!("{root}m") } else { root.clone() }),
                                "{lbl}"
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── A song's key, tempo, start, colour, profile ────────────────────────────

#[component]
fn KeyBody(index: usize, perf: PerformanceModel, on_done: EventHandler<()>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let song = perf.songs.get(index).cloned().unwrap_or_default();
    let value = if song.key.is_empty() { "C".to_string() } else { song.key.clone() };
    rsx! {
        div { style: "padding: 16px;",
            KeyGrid {
                value,
                on_pick: move |k: String| {
                    let bpm = song.bpm;
                    call!(rig, |r| r.set_setlist_entry(index as u32, k, bpm));
                    on_done.call(());
                },
            }
        }
    }
}

#[component]
fn TempoBody(index: usize, perf: PerformanceModel) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let song = perf.songs.get(index).cloned().unwrap_or_default();
    rsx! {
        div { style: "padding: 20px; display: flex; flex-direction: column; align-items: center; gap: 16px;",
            div { style: "font-size: 56px; font-weight: 750; line-height: 1; font-variant-numeric: tabular-nums;",
                if song.bpm > 0 { "{song.bpm}" } else { "—" }
                span { style: "font-size: 15px; color: {INK_3}; margin-left: 6px;", "BPM" }
            }
            div { style: "display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 8px; width: 100%;",
                for d in [-5_i32, -1, 1, 5] {
                    {
                        let rig = rig.clone();
                        let key = song.key.clone();
                        let bpm = song.bpm;
                        rsx! {
                            button {
                                key: "{d}",
                                style: "min-height: 56px; border-radius: {R}; border: 1px solid {RULE_STRONG}; background: transparent; color: {INK}; font-size: 18px; font-weight: 650; font-family: {FONT}; cursor: pointer;",
                                onclick: move |_| {
                                    let next = ((if bpm == 0 { 100 } else { bpm }) as i32 + d).max(30) as u32;
                                    let key = key.clone();
                                    call!(rig, |r| r.set_setlist_entry(index as u32, key, next));
                                },
                                if d > 0 { "+{d}" } else { "−{-d}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn StartBody(index: usize, perf: PerformanceModel, lib: LibraryModel, on_done: EventHandler<()>) -> Element {
    let song = perf.songs.get(index).cloned().unwrap_or_default();
    let lib_song = lib.songs.iter().find(|s| s.name.eq_ignore_ascii_case(&song.name)).cloned().unwrap_or_default();
    let current = lib_song.start_part.clone();
    rsx! {
        div { style: "padding: 10px 14px 16px; display: flex; flex-direction: column; gap: 6px;",
            Cell { title: "The profile's default".to_string(), on: current.is_empty(), song: song.name.clone(), part: String::new(), on_done }
            for p in lib_song.parts.iter().cloned() {
                Cell { key: "{p}", title: p.clone(), on: p.eq_ignore_ascii_case(&current), song: song.name.clone(), part: p.clone(), on_done }
            }
        }
    }
}

/// A choice in a list of them: the one on is ringed.
#[component]
fn Cell(title: String, on: bool, song: String, part: String, on_done: EventHandler<()>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    rsx! {
        button {
            style: "min-height: 48px; padding: 6px 12px; text-align: left; border-radius: {R}; background: {pick(on, UP, SHEET)}; border: {ring(on, 1.5)}; color: {INK}; font-size: 15px; font-weight: {pick(on, 700, 560)}; font-family: {FONT}; cursor: pointer;",
            onclick: move |_| {
                let (song, part) = (song.clone(), part.clone());
                call!(rig, |r| r.set_song_start_part(song, part));
                on_done.call(());
            },
            "{title}"
        }
    }
}

#[component]
fn ColourBody(index: usize, perf: PerformanceModel, on_done: EventHandler<()>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let song = perf.songs.get(index).cloned().unwrap_or_default();
    let chosen = song.colour.clone();
    let auto = name_colour(&song.name);
    let name = song.name.clone();
    rsx! {
        div { style: "padding: 16px; display: flex; flex-direction: column; gap: 14px;",
            button {
                style: "display: flex; align-items: center; gap: 12px; min-height: 56px; padding: 0 14px; border-radius: {R}; border: {ring(chosen.is_empty(), 2.0)}; background: {pick(chosen.is_empty(), UP, SHEET)}; color: {INK}; text-align: left; font-family: {FONT}; cursor: pointer;",
                onclick: {
                    let rig = rig.clone();
                    let name = name.clone();
                    move |_| {
                        let name = name.clone();
                        call!(rig, |r| r.set_song_colour(name, String::new()));
                        on_done.call(());
                    }
                },
                span { style: "width: 28px; height: 28px; border-radius: 7px; background: {auto}; flex-shrink: 0;" }
                span { style: "flex: 1; display: flex; flex-direction: column;",
                    span { style: "font-size: 15px; font-weight: 650;", "From its name" }
                    span { style: "font-size: 13px; color: {INK_3};", "Always the same for “{song.name}”" }
                }
            }
            div { style: "display: grid; grid-template-columns: repeat(7, minmax(0, 1fr)); gap: 8px;",
                for c in SONG_PALETTE {
                    {
                        let on = chosen.eq_ignore_ascii_case(c);
                        let (rig, name) = (rig.clone(), name.clone());
                        rsx! {
                            button {
                                key: "{c}",
                                "aria-label": "Colour {c}",
                                style: "height: 44px; border-radius: 10px; border: {pick(on, ring(true, 3.0), NO_SHADOW.to_string())}; background: {c}; box-sizing: border-box; cursor: pointer;",
                                onclick: move |_| {
                                    let name = name.clone();
                                    call!(rig, |r| r.set_song_colour(name, c.to_string()));
                                    on_done.call(());
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ProfileBody(song: Option<usize>, perf: PerformanceModel, lib: LibraryModel, on_done: EventHandler<()>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let set_index = perf.setlist_index;
    let set_profile = lib.setlists.get(set_index as usize).map(|s| s.profile.clone()).unwrap_or_default();
    let song_name = song.and_then(|i| perf.songs.get(i)).map(|s| s.name.clone());
    let current = match &song_name {
        Some(n) => lib.songs.iter().find(|s| s.name.eq_ignore_ascii_case(n)).map(|s| s.profile.clone()).unwrap_or_default(),
        None => set_profile.clone(),
    };
    // What it follows when it has none of its own.
    let fallback = match &song_name {
        Some(_) => if set_profile.is_empty() { perf.profile_name.clone() } else { set_profile.clone() },
        None => lib.profiles.iter().find(|p| p.active).map(|p| p.name.clone()).unwrap_or_else(|| perf.profile_name.clone()),
    };
    let most = lib.profiles.iter().flat_map(|p| p.stacks.iter().map(move |st| p.patch_list.iter().filter(|x| &x.stack == st).count())).max().unwrap_or(1).max(1);
    let mut profiles = lib.profiles.clone();
    profiles.sort_by_key(|p| !p.name.eq_ignore_ascii_case(&current));
    let set = move |rig: Option<RigClient>, song_name: Option<String>, name: String| match song_name {
        Some(s) => call!(rig, |r| r.set_song_profile(s, name)),
        None => call!(rig, |r| r.set_setlist_profile(set_index, name)),
    };
    let inherit_profile = lib.profiles.iter().find(|p| p.name.eq_ignore_ascii_case(&fallback)).cloned();
    rsx! {
        div { style: "display: flex; flex-direction: column;",
            ProfileCell {
                name: if song_name.is_some() { "Setlist default".to_string() } else { "Rig default".to_string() },
                of: fallback.clone(),
                inherit: true,
                profile: inherit_profile,
                on: current.is_empty(),
                most,
                onclick: {
                    let (rig, song_name) = (rig.clone(), song_name.clone());
                    move |()| {
                        set(rig.clone(), song_name.clone(), String::new());
                        on_done.call(());
                    }
                },
            }
            // The default stands apart: a band of the desk under it.
            div { style: "height: 8px; background: {DESK}; border-bottom: 1px solid {RULE};" }
            for p in profiles {
                {
                    let (rig, song_name) = (rig.clone(), song_name.clone());
                    let name = p.name.clone();
                    rsx! {
                        ProfileCell {
                            key: "{p.name}",
                            name: p.name.clone(),
                            of: String::new(),
                            inherit: false,
                            on: current.eq_ignore_ascii_case(&p.name),
                            profile: Some(p.clone()),
                            most,
                            onclick: move |()| {
                                set(rig.clone(), song_name.clone(), name.clone());
                                on_done.call(());
                            },
                        }
                    }
                }
            }
        }
    }
}

/// A profile in the list: its name, what it opens on, and its stacks drawn
/// small at the right — a column per stack, a block per patch.
#[component]
fn ProfileCell(name: String, of: String, inherit: bool, profile: Option<ProfileEntry>, on: bool, most: usize, onclick: EventHandler<()>) -> Element {
    let stacks: Vec<(String, usize)> = profile
        .as_ref()
        .map(|p| p.stacks.iter().map(|st| (st.clone(), p.patch_list.iter().filter(|x| &x.stack == st).count())).filter(|x| x.1 > 0).collect())
        .unwrap_or_default();
    let first = profile.as_ref().and_then(|p| stacks.first().and_then(|(st, _)| p.patch_list.iter().find(|x| &x.stack == st)).map(|x| x.name.clone())).unwrap_or_default();
    let meta: Vec<String> = [of.clone(), first].into_iter().filter(|x| !x.is_empty()).collect();
    let colour = name_colour(&name).to_string();
    rsx! {
        button {
            style: "width: 100%; min-height: 48px; padding: 4px 16px; display: flex; align-items: center; gap: 12px; text-align: left; justify-content: flex-start; border: none; {rule_below(!inherit)} background: {pick(on, UP, CLEAR)}; color: {INK}; font-family: {FONT}; cursor: pointer;",
            onclick: move |_| onclick.call(()),
            span { style: "width: 22px; display: flex; justify-content: center; flex-shrink: 0;",
                if inherit {
                    svg { width: "18", height: "18", view_box: "0 0 16 16",
                        path { d: "M4 2v5.5a2.5 2.5 0 0 0 2.5 2.5H13", fill: "none", stroke: INK_3, stroke_width: "1.5", stroke_linecap: "round" }
                        path { d: "M10 7l3 3-3 3", fill: "none", stroke: INK_3, stroke_width: "1.5", stroke_linecap: "round", stroke_linejoin: "round" }
                    }
                } else {
                    ProfileIcon { name: name.clone(), colour: colour.clone(), size: 18 }
                }
            }
            span { style: "flex-shrink: 0; font-size: 15px; font-weight: {pick(on, 700, 600)}; font-style: {pick(inherit, \"italic\", \"normal\")}; color: {pick(on, INK, INK_2)}; white-space: nowrap;", "{name}" }
            span { style: "flex: 1; min-width: 0; text-align: right; font-size: 11.5px; color: {INK_3}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{meta.join(\" · \")}" }
            if on {
                svg { width: "16", height: "16", view_box: "0 0 14 14", style: "flex-shrink: 0;",
                    path { d: "M2.5 7.5 5.5 10.5 11.5 3.5", fill: "none", stroke: INK, stroke_width: "1.8", stroke_linecap: "round", stroke_linejoin: "round" }
                }
            }
            span { style: "display: flex; align-items: flex-end; justify-content: flex-end; gap: 3px; width: 45px; height: {most * 5}px; flex-shrink: 0;",
                for (st, n) in stacks {
                    {
                        let (tape, _) = crate::perform::folder_color(&st);
                        rsx! {
                            span { key: "{st}", style: "display: flex; flex-direction: column; gap: 1px;",
                                for k in 0..n {
                                    span { key: "{k}", style: "width: 6px; height: 4px; border-radius: 1px; background: {tape}; opacity: {pick(on, 1.0, 0.75)};" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── A part's patch ─────────────────────────────────────────────────────────

/// Every stack of the part's profile and its patches; "Keep what plays"
/// clears the part's own.
#[component]
fn PatchBody(part: usize, perf: PerformanceModel, lib: LibraryModel, on_done: EventHandler<()>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let p = perf.parts.get(part).cloned().unwrap_or_default();
    let profile_name = if p.profile.is_empty() { perf.profile_name.clone() } else { p.profile.clone() };
    let profile = lib.profiles.iter().find(|x| x.name.eq_ignore_ascii_case(&profile_name)).cloned().unwrap_or_default();
    let set = move |rig: Option<RigClient>, part: String, patch: String| call!(rig, |r| r.set_part_patch(part, patch));
    rsx! {
        div { style: "padding: 10px 14px 16px; display: flex; flex-direction: column; gap: 12px;",
            PatchCell { title: "Keep what plays".to_string(), on: p.patch.is_empty(), onclick: {
                let (rig, name) = (rig.clone(), p.name.clone());
                move |()| { set(rig.clone(), name.clone(), String::new()); on_done.call(()); }
            } }
            for st in profile.stacks.iter().filter(|st| profile.patch_list.iter().any(|x| &x.stack == *st)).cloned() {
                {
                    let (tape, _) = crate::perform::folder_color(&st);
                    let patches: Vec<String> = profile.patch_list.iter().filter(|x| x.stack == st).map(|x| x.name.clone()).collect();
                    rsx! {
                        div { key: "{st}",
                            div { style: "display: flex; align-items: center; gap: 8px; margin-bottom: 6px;",
                                span { style: "width: 10px; height: 10px; border-radius: 3px; background: {tape};" }
                                span { style: "font-size: 11px; font-weight: 750; letter-spacing: 0.08em; text-transform: uppercase; color: {lift(tape)};", "{st}" }
                            }
                            div { style: "display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 6px;",
                                for name in patches {
                                    {
                                        let (rig, part_name, patch) = (rig.clone(), p.name.clone(), name.clone());
                                        rsx! {
                                            PatchCell { key: "{name}", title: name.clone(), on: p.patch.eq_ignore_ascii_case(&name), onclick: move |()| {
                                                set(rig.clone(), part_name.clone(), patch.clone());
                                                on_done.call(());
                                            } }
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
}

#[component]
fn PatchCell(title: String, on: bool, onclick: EventHandler<()>) -> Element {
    rsx! {
        button {
            style: "min-height: 48px; padding: 6px 12px; text-align: left; border-radius: {R}; background: {pick(on, UP, SHEET)}; border: {ring(on, 1.5)}; color: {INK}; font-size: 15px; font-weight: {pick(on, 700, 560)}; font-family: {FONT}; cursor: pointer;",
            onclick: move |_| onclick.call(()),
            "{title}"
        }
    }
}
