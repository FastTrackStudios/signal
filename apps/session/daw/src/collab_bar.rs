//! The collaboration control in the top bar: one button, showing who is
//! here as a row of avatars (or, alone, a share glyph), that opens the
//! session card — share this set or join someone's, the people and the song
//! each is on, whether everyone plays on their own or together, the
//! invite, and leaving.
//!
//! In a page (the public demo) the page has already joined its set by its
//! link: the card is the people, the transport, the invite (the page's own
//! address) and leaving.
//!
//! Natively it also starts a session from the environment, for the two-instance demo
//! and tests: `FTS_COLLAB_HOST=1` shares the song once it is open and
//! writes the ticket to `FTS_COLLAB_TICKET` (default: the temp dir's
//! `fts-session-ticket`); `FTS_COLLAB_JOIN` joins — a ticket, or a path
//! to a file holding one (waiting for the file to appear).

use std::time::Duration;

use dioxus::prelude::*;

use crate::shell::{ACCENT, BAR_BG, DIM, Density, RULE, TEXT};

/// Where an env-started host leaves its ticket.
#[cfg(feature = "native")]
fn ticket_path() -> std::path::PathBuf {
    std::env::var_os("FTS_COLLAB_TICKET").map_or_else(
        || std::env::temp_dir().join("fts-session-ticket"),
        std::path::PathBuf::from,
    )
}

#[cfg(feature = "native")]
fn display_name() -> String {
    std::env::var("FTS_COLLAB_NAME")
        .ok()
        .or_else(|| std::env::var("USER").ok())
        .unwrap_or_else(|| "Someone".to_owned())
}

#[component]
pub fn CollabBar() -> Element {
    let mut status = use_signal(crate::collab::status);
    let mut error = use_signal(|| None::<String>);
    // A playground's time left, in whole seconds (ticks the countdown).
    let mut resets_in = use_signal(|| None::<u64>);
    #[cfg(feature = "native")]
    let mut joining = use_signal(String::new);
    let mut people = use_signal(crate::ghosts::everyone);
    // Whether a session is being joined (a page joins in the background).
    let mut session_joining = use_signal(crate::collab::joining);
    let mut open = use_signal(|| false);
    let mut copied = use_signal(|| false);
    let density = crate::shell::use_density();

    #[cfg(feature = "native")]
    // The environment's session, once the song is up.
    use_hook(move || {
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(1500));
            // The song's own record of its edits, from the first moment —
            // shared or not.
            let task_set = crate::collab::TaskSet::from_env();
            if !crate::collab::env_set("FTS_COLLAB_JOIN")
                && task_set.is_none()
                && let Err(e) = crate::collab::open_local()
            {
                tracing::warn!(collab.error = %e, "collab: the song's history is not being kept");
            }
            if let Some(set) = task_set {
                match crate::collab::join_task(&set, display_name()) {
                    Ok(()) => {
                        tracing::info!(collab.setlist = %set.setlist, "collab: in the set Task keeps")
                    }
                    Err(e) => {
                        tracing::warn!(collab.error = %e, "collab: could not join the set Task keeps")
                    }
                }
            } else if crate::collab::env_set("FTS_COLLAB_HOST") {
                match crate::collab::host(display_name()) {
                    Ok(ticket) => {
                        let path = ticket_path();
                        if let Err(e) = std::fs::write(&path, &ticket) {
                            tracing::warn!(collab.ticket_error = %e, "collab: the ticket was not written");
                        }
                        tracing::info!(collab.ticket_file = %path.display(), "collab: hosting");
                    }
                    Err(e) => tracing::warn!(collab.error = %e, "collab: could not host"),
                }
            } else if let Some(join) = std::env::var_os("FTS_COLLAB_JOIN").filter(|v| !v.is_empty())
            {
                let join = join.to_string_lossy().into_owned();
                let ticket = if join.starts_with("fts-session:") {
                    Some(join)
                } else {
                    // A file the host writes: wait for it.
                    (0..120).find_map(|_| {
                        let t = std::fs::read_to_string(&join)
                            .ok()
                            .filter(|t| t.starts_with("fts-session:"));
                        if t.is_none() {
                            std::thread::sleep(Duration::from_millis(500));
                        }
                        t
                    })
                };
                match ticket.map(|t| crate::collab::join(t.trim(), display_name())) {
                    Some(Ok(())) => tracing::info!("collab: joined"),
                    Some(Err(e)) => tracing::warn!(collab.error = %e, "collab: could not join"),
                    None => tracing::warn!("collab: no ticket to join"),
                }
            }
        });
    });

    // The status, kept current (peers come and go with nothing clicked).
    use_future(move || async move {
        loop {
            architect::platform::sleep(Duration::from_millis(500)).await;
            let now = crate::collab::status();
            let left = now.as_ref().and_then(|s| s.resets_at).map(|at| {
                at.saturating_duration_since(architect::platform::Instant::now())
                    .as_secs()
            });
            if *resets_in.peek() != left {
                resets_in.set(left);
            }
            if *status.peek() != now {
                status.set(now);
            }
            let now_joining = crate::collab::joining();
            if *session_joining.peek() != now_joining {
                session_joining.set(now_joining);
            }
            let why = crate::collab::last_error();
            if *error.peek() != why {
                error.set(why);
            }
            let here = crate::ghosts::everyone();
            if *people.peek() != here {
                people.set(here);
            }
        }
    });

    let live = status();
    let others = people();
    // The button: who is here, you first — or, alone, a share glyph.
    let face = match &live {
        Some(l) => {
            let mut faces = vec![(l.name.clone(), l.color)];
            faces.extend(others.iter().map(|p| (p.name.clone(), p.color)));
            let extra = faces.len().saturating_sub(4);
            faces.truncate(4);
            rsx! {
                span { style: "width:7px; height:7px; border-radius:4px; background:#4ac26b; flex:none;" }
                div {
                    style: "display:flex; align-items:center;",
                    for (i, (name, color)) in faces.into_iter().enumerate() {
                        Avatar { name, color, size: 20.0, overlap: i > 0 }
                    }
                    if extra > 0 {
                        span { style: "margin-left:4px; font-size:11px; color:{DIM};", "+{extra}" }
                    }
                }
            }
        }
        None if session_joining() => rsx! {
            span { style: "font-size:12px; font-weight:600; color:{DIM};", "Joining…" }
        },
        None => rsx! {
            ShareGlyph {}
            if density == Density::Full {
                span { style: "font-size:12px; font-weight:600; color:{TEXT};", "Share" }
            }
        },
    };
    let title = match &live {
        Some(l) => format!(
            "{} · {}",
            if l.hosting { "Sharing" } else { "Joined" },
            who(l.peers)
        ),
        None => "Share this set, or join someone's".to_owned(),
    };
    rsx! {
        div {
            style: "position:relative; flex:none; height:100%; display:flex;",
            onmousedown: move |event| event.stop_propagation(),
            // Flat, the bar's full height: a part of the bar between its
            // rules, not a card on it.
            button {
                title: "{title}",
                style: "height:100%; box-sizing:border-box; display:flex; align-items:center; gap:7px; \
                        padding:0 12px; border:none; background:transparent; cursor:pointer;",
                onclick: move |_| open.toggle(),
                {face}
                // A playground starts over on a timer: how long this run
                // has left, so nobody is surprised by it.
                if let Some(left) = resets_in() {
                    span {
                        style: "font-size:11px; font-variant-numeric:tabular-nums; color:{DIM};",
                        "{clock(left)}"
                    }
                }
                if error().is_some() {
                    span { style: "width:7px; height:7px; border-radius:4px; background:#e3b341;" }
                }
            }
            if open() {
                div {
                    style: "position:absolute; right:0; top:34px; z-index:40; width:300px; \
                            box-sizing:border-box; padding:14px; display:flex; flex-direction:column; \
                            gap:12px; background:{BAR_BG}; border:1px solid {RULE}; border-radius:12px; \
                            box-shadow:0 12px 32px rgba(0,0,0,0.55); color:{TEXT};",
                    // Heading: what this is, and a close.
                    div {
                        style: "display:flex; align-items:center; justify-content:space-between;",
                        div {
                            style: "display:flex; flex-direction:column; gap:2px;",
                            span { style: "font-size:13px; font-weight:700;",
                                match &live {
                                    Some(l) if l.hosting => rsx! { "Sharing {l.set}" },
                                    Some(l) => rsx! { "In {l.set}" },
                                    None => rsx! { "Play together" },
                                }
                            }
                            span { style: "font-size:11px; color:{DIM};",
                                match (&live, resets_in()) {
                                    (Some(l), Some(left)) => rsx! { "{who(l.peers)} · starts over in {clock(left)}" },
                                    (Some(l), None) => rsx! { "{who(l.peers)}" },
                                    (None, _) => rsx! { "Share this set, or join someone's" },
                                }
                            }
                        }
                        button {
                            style: "width:22px; height:22px; border:none; border-radius:6px; background:transparent; \
                                    color:{DIM}; font-size:14px; cursor:pointer;",
                            onclick: move |_| open.set(false),
                            "×"
                        }
                    }
                    match live.clone() {
                        #[cfg(feature = "native")]
                        None => rsx! {
                            button {
                                style: primary(),
                                onclick: move |_| crate::collab::host_in_background(display_name()),
                                "Share this set"
                            }
                            Section { label: "Join" }
                            div {
                                style: "display:flex; gap:6px;",
                                input {
                                    style: "flex:1; min-width:0; height:26px; box-sizing:border-box; padding:0 8px; \
                                            border-radius:6px; border:1px solid {RULE}; background:#0f1012; \
                                            color:{TEXT}; font-size:11px;",
                                    placeholder: "paste a ticket",
                                    value: "{joining}",
                                    oninput: move |e| joining.set(e.value()),
                                    onmousedown: move |e| { crate::keys::set_editing(true); e.stop_propagation(); },
                                }
                                button {
                                    style: secondary(),
                                    onclick: move |_| crate::collab::join_in_background(joining.peek().trim().to_owned(), display_name()),
                                    "Join"
                                }
                            }
                        },
                        // A page joins its set by its link, or is not in one.
                        #[cfg(not(feature = "native"))]
                        None => rsx! {
                            span { style: "font-size:12px; color:{DIM};", "Not in a session — open a live link to join one." }
                        },
                        Some(l) => {
                            let shared = l.shared_transport;
                            // In a page, the invite is the page itself.
                            #[cfg(not(feature = "native"))]
                            let ticket = invite_link().unwrap_or(l.ticket.clone());
                            #[cfg(feature = "native")]
                            let ticket = l.ticket.clone();
                            let current = crate::open::current_song().and_then(|p| crate::collab::key_of(&p));
                            rsx! {
                                Section { label: "People" }
                                div {
                                    style: "display:flex; flex-direction:column; gap:6px;",
                                    Row { name: l.name.clone(), color: l.color, note: "you".to_owned() }
                                    for p in others.iter().cloned() {
                                        Row {
                                            name: p.name,
                                            color: p.color,
                                            note: if current.as_deref() == Some(p.song.as_str()) { "here".to_owned() } else { format!("on {}", p.song) },
                                        }
                                    }
                                }
                                Section { label: "Transport" }
                                div {
                                    style: "display:flex; gap:2px; padding:2px; background:#0f1012; \
                                            border:1px solid {RULE}; border-radius:7px;",
                                    button {
                                        style: "{half(!shared)}",
                                        title: "Everyone plays on their own; the others' playheads show faintly",
                                        onclick: move |_| { crate::collab::set_shared_transport(false); status.set(crate::collab::status()); },
                                        "Apart"
                                    }
                                    button {
                                        style: "{half(shared)}",
                                        title: "One transport: play here and it plays for everyone",
                                        onclick: move |_| { crate::collab::set_shared_transport(true); status.set(crate::collab::status()); },
                                        "Together"
                                    }
                                }
                                Section { label: "Invite" }
                                div {
                                    style: "display:flex; gap:6px;",
                                    input {
                                        style: "flex:1; min-width:0; height:26px; box-sizing:border-box; padding:0 8px; \
                                                border-radius:6px; border:1px solid {RULE}; background:#0f1012; \
                                                color:{DIM}; font-size:10px; font-family:ui-monospace, monospace;",
                                        readonly: true,
                                        value: "{ticket}",
                                        onmousedown: move |e| e.stop_propagation(),
                                    }
                                    button {
                                        style: secondary(),
                                        onclick: move |_| copied.set(copy(&ticket)),
                                        if copied() { "Copied" } else { "Copy" }
                                    }
                                }
                                button {
                                    style: "height:28px; border-radius:7px; border:1px solid #5a2a2a; background:transparent; \
                                            color:#f28b82; font-size:12px; font-weight:600; cursor:pointer;",
                                    onclick: move |_| {
                                        crate::collab::leave();
                                        status.set(None);
                                        copied.set(false);
                                    },
                                    if l.hosting { "Stop sharing" } else { "Leave" }
                                }
                            }
                        }
                    }
                    if let Some(why) = error() {
                        span { style: "font-size:11px; color:#e3b341;", "{why}" }
                    }
                }
            }
        }
    }
}

/// Seconds as `m:ss`.
fn clock(seconds: u64) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn who(peers: usize) -> String {
    match peers {
        0 => "Just you so far".to_owned(),
        1 => "1 other here".to_owned(),
        n => format!("{n} others here"),
    }
}

fn primary() -> String {
    format!(
        "height:30px; border-radius:7px; border:none; background:{ACCENT}; color:#0b0c0e; \
         font-size:12px; font-weight:700; cursor:pointer;"
    )
}

fn secondary() -> String {
    format!(
        "height:26px; padding:0 10px; border-radius:6px; border:1px solid {RULE}; background:transparent; \
         color:{TEXT}; font-size:11px; font-weight:600; cursor:pointer;"
    )
}

fn half(on: bool) -> String {
    let (bg, fg) = if on {
        (ACCENT, "#0b0c0e")
    } else {
        ("transparent", DIM)
    };
    format!(
        "flex:1; height:24px; border:none; border-radius:5px; background:{bg}; color:{fg}; \
         font-size:12px; font-weight:600; cursor:pointer;"
    )
}

/// Put the ticket on the clipboard (macOS: `pbcopy`). Whether it went.
/// This page's address, as an invite: without the name it was opened
/// with, which is this visitor's, not the next one's.
#[cfg(not(feature = "native"))]
fn invite_link() -> Option<String> {
    let href = web_sys::window()?.location().href().ok()?;
    let url = web_sys::Url::new(&href).ok()?;
    url.search_params().delete("name");
    Some(url.href())
}

/// The invite, onto the browser's clipboard.
#[cfg(not(feature = "native"))]
fn copy(text: &str) -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    let _ = window.navigator().clipboard().write_text(text);
    true
}

#[cfg(feature = "native")]
fn copy(text: &str) -> bool {
    use std::io::Write as _;
    #[cfg(target_os = "macos")]
    let child = std::process::Command::new("pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn();
    #[cfg(not(target_os = "macos"))]
    let child: std::io::Result<std::process::Child> =
        Err(std::io::Error::other("no clipboard here yet"));
    let Ok(mut child) = child else { return false };
    let wrote = child
        .stdin
        .take()
        .is_some_and(|mut stdin| stdin.write_all(text.as_bytes()).is_ok());
    child.wait().is_ok_and(|s| s.success()) && wrote
}

/// A small caps heading in the card.
#[component]
fn Section(label: &'static str) -> Element {
    rsx! {
        span {
            style: "margin-bottom:-6px; font-size:9px; letter-spacing:0.9px; font-weight:700; color:{DIM};",
            "{label.to_uppercase()}"
        }
    }
}

/// A person in the card: avatar, name, where they are.
#[component]
fn Row(name: String, color: u32, note: String) -> Element {
    rsx! {
        div {
            style: "display:flex; align-items:center; gap:8px;",
            Avatar { name: name.clone(), color, size: 22.0, overlap: false }
            span { style: "flex:1; min-width:0; font-size:12px; font-weight:600; overflow:hidden; \
                           text-overflow:ellipsis; white-space:nowrap;", "{name}" }
            span { style: "font-size:11px; color:{DIM}; white-space:nowrap;", "{note}" }
        }
    }
}

/// A person's initial on their colour.
#[component]
pub fn Avatar(name: String, color: u32, size: f64, overlap: bool) -> Element {
    let (r, g, b) = ((color >> 16) & 0xff, (color >> 8) & 0xff, color & 0xff);
    let initial = name
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default();
    let margin = if overlap { -size * 0.3 } else { 0.0 };
    let font = (size * 0.5).round();
    let radius = size / 2.0;
    rsx! {
        div {
            title: "{name}",
            style: "flex:none; width:{size}px; height:{size}px; border-radius:{radius}px; margin-left:{margin}px; \
                    box-sizing:border-box; background:rgb({r},{g},{b}); border:2px solid #0f1012; \
                    display:flex; align-items:center; justify-content:center; \
                    color:#101114; font-size:{font}px; font-weight:700; line-height:1;",
            "{initial}"
        }
    }
}

/// Two people — the share button's glyph when no one is here yet.
#[component]
fn ShareGlyph() -> Element {
    rsx! {
        svg {
            width: "18",
            height: "14",
            view_box: "0 0 22 16",
            fill: "none",
            stroke: TEXT,
            stroke_width: "1.6",
            stroke_linecap: "round",
            circle { cx: "8", cy: "5", r: "3" }
            path { d: "M2.5 15 C2.5 11 5 9.5 8 9.5 C11 9.5 13.5 11 13.5 15" }
            path { d: "M18 4 V10 M15 7 H21" }
        }
    }
}

/// Who else is on a song: a coloured initial each, for its tab.
#[component]
pub fn PeerDots(project: String) -> Element {
    let mut here = use_signal(Vec::<(String, u32)>::new);
    let watching = project.clone();
    use_future(move || {
        let watching = watching.clone();
        async move {
            loop {
                let now = crate::ghosts::on_song(&watching);
                if *here.peek() != now {
                    here.set(now);
                }
                architect::platform::sleep(Duration::from_millis(300)).await;
            }
        }
    });
    let people = here();
    if people.is_empty() {
        return rsx! {};
    }
    rsx! {
        // In the tab's corner, over whatever is there: a tab in a narrow
        // window has no room to spare in its row.
        div {
            style: "position:absolute; top:1px; right:2px; display:flex; align-items:center; \
                    pointer-events:none;",
            for (i, (name, color)) in people.into_iter().enumerate() {
                {
                    let (r, g, b) = ((color >> 16) & 0xff, (color >> 8) & 0xff, color & 0xff);
                    let initial = name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
                    let overlap = if i == 0 { 0 } else { -5 };
                    rsx! {
                        div {
                            title: "{name}",
                            style: "width:14px; height:14px; border-radius:7px; margin-left:{overlap}px; \
                                    background:rgb({r},{g},{b}); border:1.5px solid #0f1012; \
                                    display:flex; align-items:center; justify-content:center; \
                                    color:#101114; font-size:8px; font-weight:700; line-height:1;",
                            "{initial}"
                        }
                    }
                }
            }
        }
    }
}

#[cfg(any(feature = "native", feature = "web"))]
/// Open the song the shared transport asks for — playing together, someone
/// else picked another song of the set.
///
/// Remote or Cue, it also follows the driven system: a tab picked in REAPER
/// itself becomes the song on screen here (it is already current there, so
/// nothing is selected back).
pub fn use_follow_song(mut setlist: Signal<crate::setlist::Setlist>) {
    use_future(move || async move {
        loop {
            architect::platform::sleep(Duration::from_millis(100)).await;
            if !crate::audio_mode::owns_project()
                && let Some(remote) = crate::open::current_song()
            {
                let index = {
                    let list = setlist.peek();
                    (list.current().is_some_and(|s| s.project != remote))
                        .then(|| list.songs.iter().position(|s| s.project == remote))
                        .flatten()
                };
                if let Some(index) = index {
                    let at = crate::engine::Transport::shared().map_or(0.0, |t| t.read().0);
                    setlist.write().pick(index, at);
                }
            }
            let Some(project) = crate::collab::take_song_request() else {
                continue;
            };
            let index = setlist
                .peek()
                .songs
                .iter()
                .position(|s| s.project == project);
            let at = crate::engine::Transport::shared().map_or(0.0, |t| t.read().0);
            if let Some(index) = index
                && setlist.write().pick(index, at).is_some()
            {
                crate::open::switch_song(&project);
            }
        }
    });
}
