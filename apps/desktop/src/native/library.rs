//! The library editor: an org's lists of songs, made and kept here.
//!
//! Two kinds of list, one editor ([`ListKind`]):
//!
//! - **Song lists** — songs gathered to pick from: "Worship Tracks", "Hymns",
//!   "Christmas". A shelf, in whatever order is handy.
//! - **Setlists** — a service or a show, in the order it is played: "JHM
//!   Sunday". Made by picking from the song lists (or from every song).
//!
//! Three panes: the lists, the one being edited, and the songs to add to
//! it — from every song in the library, or from one of the song lists. The
//! editor changes its own copy at once, and a saver sends each list's
//! whole order to Task behind it ([`Library::set_songs`]), one list at a
//! time and the latest order only, so quick edits never race each other
//! and a slow connection never holds a tap up.

use std::collections::HashMap;

use dioxus::prelude::*;
use lucide_dioxus::{ArrowDown, ArrowUp, ChevronLeft, ListMusic, Play, Plus, Search, X};
use session_daw::stream_set::{Library, LibrarySetlist, LibrarySong, ListKind, with_library};

use super::start::{
    ACCENT, BAR, BG, Chip, DIM, Heading, Load, Pill, RULE, TEXT, TOP, WARN, brief, off_thread,
};

/// Where the songs to add come from.
#[derive(Clone, PartialEq)]
enum From {
    /// Every song in the library.
    Everything,
    /// One song list (by id).
    List(String),
}

/// The org's lists, to make, change, delete and play.
#[component]
pub fn LibraryEditor(
    library: Library,
    /// The lists as the start screen last read them: shown at once, and
    /// read again behind them.
    lists: Vec<LibrarySetlist>,
    /// The list to open on (by id); the first when `None`.
    #[props(default)]
    open: Option<String>,
    /// Make a new list of this kind as the editor opens.
    #[props(default)]
    create_first: Option<ListKind>,
    on_back: EventHandler<()>,
    on_play: EventHandler<LibrarySetlist>,
) -> Element {
    let mut lists = use_signal(|| lists);
    let mut songs = use_signal(|| Load::<Vec<(LibrarySong, bool)>>::Waiting);
    let mut selected = use_signal(|| {
        open.clone()
            .or_else(|| lists.peek().first().map(|l| l.id.clone()))
    });
    let mut from = use_signal(|| From::Everything);
    let mut query = use_signal(String::new);
    let mut renaming = use_signal(|| None::<String>);
    let mut deleting = use_signal(|| false);
    let mut problem = use_signal(|| None::<String>);
    // Each list's order as last changed here, and as last saved: the saver
    // sends a list when the two differ.
    let mut changed = use_signal(HashMap::<String, u64>::new);
    let mut saved = use_signal(HashMap::<String, u64>::new);

    // The library's songs, and its lists afresh.
    {
        let library = library.clone();
        use_hook(move || {
            let for_songs = library.clone();
            spawn(async move {
                match off_thread(move || {
                    with_library(&for_songs, |l| async move { l.songs().await })
                })
                .await
                {
                    Some(Ok(list)) => songs.set(Load::Ready(list)),
                    Some(Err(e)) => songs.set(Load::Failed(brief(&e))),
                    None => {}
                }
            });
            spawn(async move {
                if let Some(Ok(fresh)) = off_thread(move || {
                    with_library(&library, |l| async move { l.setlists().await })
                })
                .await
                {
                    // Not while an edit here is still on its way.
                    if changed
                        .peek()
                        .iter()
                        .all(|(id, n)| saved.peek().get(id) == Some(n))
                    {
                        lists.set(fresh);
                    }
                }
            });
        });
    }

    // The saver: one list at a time, its latest order, until none is due —
    // started by an edit, never two at once.
    let mut running = use_signal(|| false);
    let save =
        {
            let library = library.clone();
            move || {
                if *running.peek() {
                    return;
                }
                running.set(true);
                let library = library.clone();
                spawn(async move {
                    loop {
                        let due = changed
                            .peek()
                            .iter()
                            .find(|(id, n)| saved.peek().get(*id) != Some(n))
                            .map(|(id, n)| (id.clone(), *n));
                        let Some((id, generation)) = due else { break };
                        let order =
                            lists.peek().iter().find(|l| l.id == id).map(|l| {
                                l.songs.iter().map(|s| s.slug.clone()).collect::<Vec<_>>()
                            });
                        if let Some(order) = order {
                            let library = library.clone();
                            let list = id.clone();
                            match off_thread(move || {
                                with_library(&library, |l| async move {
                                    l.set_songs(&list, &order).await
                                })
                            })
                            .await
                            {
                                Some(Ok(())) => problem.set(None),
                                Some(Err(e)) => problem.set(Some(brief(&e))),
                                None => {}
                            }
                        }
                        // Tried: a failure is said, and not retried in a loop.
                        saved.write().insert(id, generation);
                    }
                    running.set(false);
                });
            }
        };
    let saving = changed
        .read()
        .iter()
        .any(|(id, n)| saved.read().get(id) != Some(n));
    let mut touch = move |id: &str| {
        *changed.write().entry(id.to_owned()).or_default() += 1;
        save.clone()();
    };

    // Changing the list being edited.
    let edit = move |change: &dyn Fn(&mut Vec<LibrarySong>)| {
        let Some(id) = selected() else { return };
        if let Some(list) = lists.write().iter_mut().find(|l| l.id == id) {
            change(&mut list.songs);
        }
        touch(&id);
    };
    let create = {
        let library = library.clone();
        move |kind: ListKind| {
            let title = match kind {
                ListKind::Set => "New setlist",
                ListKind::Songs => "New song list",
            };
            let library = library.clone();
            spawn(async move {
                match off_thread(move || {
                    with_library(
                        &library,
                        |l| async move { l.create_list(title, kind).await },
                    )
                })
                .await
                {
                    Some(Ok(list)) => {
                        let id = list.id.clone();
                        lists.write().push(list);
                        selected.set(Some(id));
                        renaming.set(Some(title.to_owned()));
                        deleting.set(false);
                        // A new setlist is filled from the song lists.
                        if kind == ListKind::Set
                            && let Some(first) =
                                lists.peek().iter().find(|l| l.kind == ListKind::Songs)
                        {
                            from.set(From::List(first.id.clone()));
                        }
                    }
                    Some(Err(e)) => problem.set(Some(brief(&e))),
                    None => {}
                }
            });
        }
    };
    // Opened to make a list: made at once, named next.
    {
        let create = create.clone();
        use_hook(move || {
            if let Some(kind) = create_first {
                create(kind);
            }
        });
    }
    let rename = {
        let library = library.clone();
        move |title: String| {
            let title = title.trim().to_owned();
            renaming.set(None);
            let Some(id) = selected() else { return };
            if title.is_empty() {
                return;
            }
            if let Some(list) = lists.write().iter_mut().find(|l| l.id == id) {
                list.title.clone_from(&title);
            }
            let library = library.clone();
            spawn(async move {
                if let Some(Err(e)) = off_thread(move || {
                    with_library(
                        &library,
                        |l| async move { l.rename_list(&id, &title).await },
                    )
                })
                .await
                {
                    problem.set(Some(brief(&e)));
                }
            });
        }
    };
    let delete = {
        let library = library.clone();
        move |()| {
            let Some(id) = selected() else { return };
            if !deleting() {
                deleting.set(true);
                return;
            }
            deleting.set(false);
            lists.write().retain(|l| l.id != id);
            selected.set(lists.peek().first().map(|l| l.id.clone()));
            if from() == From::List(id.clone()) {
                from.set(From::Everything);
            }
            let library = library.clone();
            spawn(async move {
                if let Some(Err(e)) = off_thread(move || {
                    with_library(&library, |l| async move { l.delete_list(&id).await })
                })
                .await
                {
                    problem.set(Some(brief(&e)));
                }
            });
        }
    };

    // A copy of the list being edited, named after it: last week's set as
    // the start of this week's.
    let duplicate = {
        let library = library.clone();
        move |()| {
            let Some(id) = selected() else { return };
            let Some(from_list) = lists.peek().iter().find(|l| l.id == id).cloned() else {
                return;
            };
            let title = format!("{} (copy)", from_list.title);
            let library = library.clone();
            spawn(async move {
                match off_thread(move || {
                    with_library(&library, |l| async move {
                        l.duplicate_list(&from_list, &title).await
                    })
                })
                .await
                {
                    Some(Ok(copy)) => {
                        let id = copy.id.clone();
                        let title = copy.title.clone();
                        lists.write().push(copy);
                        selected.set(Some(id));
                        renaming.set(Some(title));
                        deleting.set(false);
                    }
                    Some(Err(e)) => problem.set(Some(brief(&e))),
                    None => {}
                }
            });
        }
    };
    // A song the library does not have yet, made from what was searched for
    // and put straight in the list being edited.
    let make_song = {
        let library = library.clone();
        let edit = edit.clone();
        move |title: String| {
            let title = title.trim().to_owned();
            if title.is_empty() {
                return;
            }
            let library = library.clone();
            let mut edit = edit.clone();
            spawn(async move {
                match off_thread(move || {
                    with_library(
                        &library,
                        |l| async move { l.create_song(&title, "", &[]).await },
                    )
                })
                .await
                {
                    Some(Ok(song)) => {
                        if let Load::Ready(all) = &mut *songs.write() {
                            all.push((song.clone(), false));
                            all.sort_by(|a, b| {
                                a.0.title.to_lowercase().cmp(&b.0.title.to_lowercase())
                            });
                        }
                        query.set(String::new());
                        edit(&move |list: &mut Vec<LibrarySong>| {
                            if !list.iter().any(|s| s.slug == song.slug) {
                                list.push(song.clone());
                            }
                        });
                    }
                    Some(Err(e)) => problem.set(Some(brief(&e))),
                    None => {}
                }
            });
        }
    };

    let all_lists = lists();
    let current = selected().and_then(|id| all_lists.iter().find(|l| l.id == id).cloned());
    let in_current: Vec<String> = current
        .as_ref()
        .map(|l| l.songs.iter().map(|s| s.slug.clone()).collect())
        .unwrap_or_default();
    let playable: HashMap<String, bool> = match &*songs.read() {
        Load::Ready(list) => list.iter().map(|(s, p)| (s.slug.clone(), *p)).collect(),
        _ => HashMap::new(),
    };
    // The songs to add: from the chosen source, matching the search.
    let wanted = query().trim().to_lowercase();
    let source: Load<Vec<(LibrarySong, bool)>> = match from() {
        From::Everything => songs(),
        From::List(id) => match all_lists.iter().find(|l| l.id == id) {
            Some(list) => Load::Ready(
                list.songs
                    .iter()
                    .map(|s| (s.clone(), playable.get(&s.slug).copied().unwrap_or(true)))
                    .collect(),
            ),
            None => Load::Ready(Vec::new()),
        },
    };
    let source = match source {
        Load::Ready(list) => Load::Ready(
            list.into_iter()
                .filter(|(s, _)| {
                    wanted.is_empty()
                        || s.title.to_lowercase().contains(&wanted)
                        || s.writers.iter().any(|w| w.to_lowercase().contains(&wanted))
                })
                .collect::<Vec<_>>(),
        ),
        other => other,
    };
    let song_lists: Vec<LibrarySetlist> = all_lists
        .iter()
        .filter(|l| l.kind == ListKind::Songs)
        .cloned()
        .collect();
    // What was typed, and whether the library has a song called exactly that.
    let typed = query().trim().to_owned();
    let exact = match &*songs.read() {
        Load::Ready(all) => all
            .iter()
            .any(|(s, _)| s.title.eq_ignore_ascii_case(&typed)),
        _ => true,
    };
    let status = match (problem(), saving) {
        (Some(why), _) => (WARN, format!("Not saved — {why}")),
        (None, true) => (DIM, "Saving…".to_owned()),
        (None, false) => (DIM, format!("Library · {}", library.org)),
    };

    rsx! {
        div {
            style: "position:absolute; inset:0; display:flex; flex-direction:column; background:{BG}; color:{TEXT}; \
                    font-family:system-ui, -apple-system, sans-serif;",
            // The header: back, and what is being saved.
            div {
                style: "flex:none; display:flex; align-items:center; gap:12px; padding:{TOP}px 16px 12px; \
                        border-bottom:1px solid {RULE}; background:{BAR};",
                button {
                    style: "height:36px; display:flex; align-items:center; gap:6px; padding:0 12px 0 8px; border-radius:9px; \
                            border:1px solid {RULE}; background:#1c1e22; color:{TEXT}; font-family:inherit; font-size:14px; cursor:pointer;",
                    onclick: move |_| on_back.call(()),
                    ChevronLeft { size: 18, color: "currentColor" }
                    "Back"
                }
                span { style: "font-size:17px; font-weight:700;", "Lists" }
                span { style: "flex:1; min-width:0; font-size:12px; color:{status.0}; text-align:right; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;", "{status.1}" }
            }
            div {
                style: "flex:1; min-height:0; display:flex; flex-wrap:wrap; gap:0; overflow:auto;",
                // The lists.
                Pane { width: "260px",
                    ListGroup {
                        title: "Setlists",
                        lists: all_lists.iter().filter(|l| l.kind == ListKind::Set).cloned().collect::<Vec<_>>(),
                        selected: selected(),
                        empty: "A service or a show, in the order it is played.",
                        on_pick: move |id: String| { selected.set(Some(id)); renaming.set(None); deleting.set(false); },
                        on_new: {
                            let create = create.clone();
                            move |()| create(ListKind::Set)
                        },
                    }
                    ListGroup {
                        title: "Song lists",
                        lists: song_lists.clone(),
                        selected: selected(),
                        empty: "Songs gathered to pick sets from — \"Worship Tracks\".",
                        on_pick: move |id: String| { selected.set(Some(id)); renaming.set(None); deleting.set(false); },
                        on_new: {
                            let create = create.clone();
                            move |()| create(ListKind::Songs)
                        },
                    }
                }
                // The list being edited.
                Pane { width: "420px",
                    match current.clone() {
                        None => rsx! {
                            span { style: "font-size:14px; color:{DIM}; padding:8px 4px;", "Pick a list, or make one." }
                        },
                        Some(list) => rsx! {
                            div {
                                style: "display:flex; align-items:center; gap:8px; min-height:44px;",
                                match renaming() {
                                    Some(text) => rsx! {
                                        input {
                                            style: "flex:1; min-width:0; height:40px; box-sizing:border-box; padding:0 10px; border-radius:9px; \
                                                    border:1px solid {ACCENT}; background:#101216; color:{TEXT}; font-family:inherit; font-size:17px; font-weight:650;",
                                            r#type: "text",
                                            value: "{text}",
                                            oninput: move |e| renaming.set(Some(e.value())),
                                            onkeydown: {
                                                let mut rename = rename.clone();
                                                move |e: KeyboardEvent| {
                                                    if e.key() == Key::Enter {
                                                        rename(renaming().unwrap_or_default());
                                                    }
                                                }
                                            },
                                        }
                                        Pill { label: "Save", primary: true, on_press: {
                                            let mut rename = rename.clone();
                                            move |()| rename(renaming().unwrap_or_default())
                                        } }
                                    },
                                    None => rsx! {
                                        button {
                                            style: "flex:1; min-width:0; text-align:left; border:none; background:transparent; color:{TEXT}; \
                                                    font-family:inherit; font-size:20px; font-weight:700; cursor:text; padding:4px; \
                                                    white-space:nowrap; overflow:hidden; text-overflow:ellipsis;",
                                            title: "Rename",
                                            onclick: {
                                                let title = list.title.clone();
                                                move |_| renaming.set(Some(title.clone()))
                                            },
                                            "{list.title}"
                                        }
                                    },
                                }
                            }
                            div {
                                style: "display:flex; align-items:center; gap:8px; flex-wrap:wrap;",
                                span {
                                    style: "font-size:12px; color:{DIM};",
                                    {format!("{} · {} songs", match list.kind { ListKind::Set => "Setlist", ListKind::Songs => "Song list" }, list.songs.len())}
                                }
                                div { style: "flex:1;" }
                                if !list.songs.is_empty() {
                                    button {
                                        style: "height:36px; display:flex; align-items:center; gap:6px; padding:0 14px; border-radius:18px; \
                                                border:1px solid {ACCENT}; background:{ACCENT}; color:#0b0c0e; font-family:inherit; \
                                                font-size:14px; font-weight:650; cursor:pointer;",
                                        onclick: {
                                            let list = list.clone();
                                            move |_| on_play.call(list.clone())
                                        },
                                        Play { size: 16, color: "currentColor" }
                                        "Play"
                                    }
                                }
                                Pill { label: "Duplicate", primary: false, on_press: duplicate.clone() }
                                Pill {
                                    label: if deleting() { "Tap again to delete" } else { "Delete" },
                                    primary: false,
                                    on_press: delete.clone(),
                                }
                            }
                            div {
                                style: "display:flex; flex-direction:column; border-radius:12px; overflow:hidden; background:{BAR}; border:1px solid {RULE};",
                                if list.songs.is_empty() {
                                    span {
                                        style: "padding:16px; font-size:13px; color:{DIM}; line-height:1.45;",
                                        match list.kind {
                                            ListKind::Set => "No songs yet — add them from a song list, on the right.",
                                            ListKind::Songs => "No songs yet — add them from the library, on the right.",
                                        }
                                    }
                                }
                                for (i, song) in list.songs.iter().cloned().enumerate() {
                                    OrderRow {
                                        key: "{song.slug}",
                                        number: i + 1,
                                        song: song.clone(),
                                        playable: playable.get(&song.slug).copied().unwrap_or(true),
                                        first: i == 0,
                                        last: i + 1 == list.songs.len(),
                                        on_up: {
                                            let mut edit = edit.clone();
                                            move |()| edit(&|songs: &mut Vec<LibrarySong>| if i > 0 { songs.swap(i, i - 1) })
                                        },
                                        on_down: {
                                            let mut edit = edit.clone();
                                            move |()| edit(&|songs: &mut Vec<LibrarySong>| if i + 1 < songs.len() { songs.swap(i, i + 1) })
                                        },
                                        on_remove: {
                                            let mut edit = edit.clone();
                                            move |()| edit(&|songs: &mut Vec<LibrarySong>| { if i < songs.len() { songs.remove(i); } })
                                        },
                                    }
                                }
                            }
                        },
                    }
                }
                // The songs to add.
                Pane { width: "360px",
                    Heading { label: "Add songs" }
                    div {
                        style: "display:flex; flex-wrap:wrap; gap:6px;",
                        Chip { label: "All songs", on: from() == From::Everything, on_press: move |()| from.set(From::Everything) }
                        for list in song_lists.iter().filter(|l| Some(&l.id) != selected().as_ref()).cloned() {
                            Chip {
                                key: "{list.id}",
                                label: list.title.clone(),
                                on: from() == From::List(list.id.clone()),
                                on_press: move |()| from.set(From::List(list.id.clone())),
                            }
                        }
                    }
                    div {
                        style: "position:relative; display:flex; align-items:center; gap:8px; height:40px; padding:0 10px; \
                                border-radius:10px; background:#101216; border:1px solid {RULE}; color:{DIM};",
                        Search { size: 16, color: "currentColor" }
                        input {
                            style: "flex:1; min-width:0; height:38px; border:none; background:transparent; color:{TEXT}; \
                                    font-family:inherit; font-size:15px;",
                            r#type: "text",
                            placeholder: "Search songs",
                            value: "{query}",
                            oninput: move |e| query.set(e.value()),
                        }
                    }
                    // Not in the library: made here, into the list.
                    if !typed.is_empty() && !exact && current.is_some() {
                        button {
                            style: "display:flex; align-items:center; gap:10px; min-height:44px; padding:0 12px; border-radius:10px; \
                                    border:1px dashed {ACCENT}; background:transparent; color:{TEXT}; font-family:inherit; \
                                    font-size:14px; text-align:left; cursor:pointer;",
                            onclick: {
                                let make_song = make_song.clone();
                                let typed = typed.clone();
                                move |_| make_song(typed.clone())
                            },
                            Plus { size: 16, color: ACCENT }
                            span { style: "flex:1; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;", "New song \u{201c}{typed}\u{201d}" }
                        }
                    }
                    match source {
                        Load::Waiting => rsx! { span { style: "font-size:13px; color:{DIM}; padding:6px 4px;", "Reading the library…" } },
                        Load::Failed(why) => rsx! { span { style: "font-size:13px; color:{WARN}; padding:6px 4px;", "Could not read the songs — {why}" } },
                        Load::Ready(list) => rsx! {
                            div {
                                style: "display:flex; flex-direction:column; border-radius:12px; overflow:hidden; background:{BAR}; border:1px solid {RULE};",
                                if list.is_empty() {
                                    span { style: "padding:16px; font-size:13px; color:{DIM};", "No songs match." }
                                }
                                for (i, (song, playable)) in list.into_iter().enumerate() {
                                    AddRow {
                                        key: "{song.slug}",
                                        first: i == 0,
                                        added: in_current.contains(&song.slug),
                                        can_add: current.is_some(),
                                        playable,
                                        song: song.clone(),
                                        on_add: {
                                            let mut edit = edit.clone();
                                            move |()| {
                                            let song = song.clone();
                                            edit(&move |songs: &mut Vec<LibrarySong>| {
                                                if !songs.iter().any(|s| s.slug == song.slug) {
                                                    songs.push(song.clone());
                                                }
                                            });
                                            }
                                        },
                                    }
                                }
                            }
                        },
                    }
                }
            }
        }
    }
}

/// A column of the editor: at least `width`, sharing what is left.
#[component]
fn Pane(width: String, children: Element) -> Element {
    rsx! {
        div {
            style: "flex:1 1 {width}; min-width:{width}; box-sizing:border-box; padding:16px; display:flex; \
                    flex-direction:column; gap:12px; border-right:1px solid {RULE};",
            {children}
        }
    }
}

/// One kind of list: its heading, a New button, and its lists.
#[component]
fn ListGroup(
    title: String,
    lists: Vec<LibrarySetlist>,
    selected: Option<String>,
    empty: String,
    on_pick: EventHandler<String>,
    on_new: EventHandler<()>,
) -> Element {
    rsx! {
        div {
            style: "display:flex; align-items:center; gap:8px;",
            div { style: "flex:1;", Heading { label: title } }
            button {
                style: "height:32px; display:flex; align-items:center; gap:4px; padding:0 12px; border-radius:16px; \
                        border:1px solid {RULE}; background:#1c1e22; color:{TEXT}; font-family:inherit; font-size:13px; cursor:pointer;",
                onclick: move |_| on_new.call(()),
                Plus { size: 14, color: "currentColor" }
                "New"
            }
        }
        div {
            style: "display:flex; flex-direction:column; gap:4px; margin-bottom:10px;",
            if lists.is_empty() {
                span { style: "font-size:12px; color:#6b7280; line-height:1.45; padding:2px 4px;", "{empty}" }
            }
            for list in lists {
                button {
                    key: "{list.id}",
                    style: {
                        let on = selected.as_deref() == Some(list.id.as_str());
                        let (bg, border) = if on { ("#3aa0ff1f", "#3aa0ff66") } else { ("transparent", "transparent") };
                        format!("display:flex; align-items:center; gap:10px; min-height:44px; padding:6px 10px; border-radius:10px; \
                                 border:1px solid {border}; background:{bg}; color:{TEXT}; font-family:inherit; text-align:left; cursor:pointer;")
                    },
                    onclick: {
                        let id = list.id.clone();
                        move |_| on_pick.call(id.clone())
                    },
                    div { style: "flex:none; color:{DIM}; display:flex;", ListMusic { size: 17, color: "currentColor" } }
                    span { style: "flex:1; min-width:0; font-size:14px; font-weight:600; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;", "{list.title}" }
                    span { style: "flex:none; font-size:12px; color:{DIM};", "{list.songs.len()}" }
                }
            }
        }
    }
}

/// A song in the list being edited: its place, and moving or removing it.
#[component]
fn OrderRow(
    number: usize,
    song: LibrarySong,
    playable: bool,
    first: bool,
    last: bool,
    on_up: EventHandler<()>,
    on_down: EventHandler<()>,
    on_remove: EventHandler<()>,
) -> Element {
    let rule = if first {
        "none".to_owned()
    } else {
        format!("1px solid {RULE}")
    };
    rsx! {
        div {
            style: "display:flex; align-items:center; gap:10px; min-height:52px; padding:4px 6px 4px 12px; border-top:{rule};",
            span { style: "flex:none; width:22px; font-size:13px; color:{DIM}; text-align:right;", "{number}" }
            SongWords { song, playable }
                        // Colours given outright: Blitz resolves an icon's `currentColor`
            // once, and a row moved to the end kept its arrow lit.
            IconButton { title: "Earlier", enabled: !first, on_press: on_up, ArrowUp { size: 17, color: if first { "#3a3d44" } else { TEXT } } }
            IconButton { title: "Later", enabled: !last, on_press: on_down, ArrowDown { size: 17, color: if last { "#3a3d44" } else { TEXT } } }
            IconButton { title: "Take out of the list", enabled: true, on_press: on_remove, X { size: 17, color: TEXT } }
        }
    }
}

/// A song to add: its words, and Add — or that it is in the list already.
#[component]
fn AddRow(
    song: LibrarySong,
    playable: bool,
    first: bool,
    added: bool,
    can_add: bool,
    on_add: EventHandler<()>,
) -> Element {
    let rule = if first {
        "none".to_owned()
    } else {
        format!("1px solid {RULE}")
    };
    rsx! {
        div {
            style: "display:flex; align-items:center; gap:10px; min-height:52px; padding:4px 8px 4px 12px; border-top:{rule};",
            SongWords { song, playable }
            if added {
                span { style: "flex:none; font-size:12px; color:{DIM}; padding:0 8px;", "In the list" }
            } else if can_add {
                button {
                    style: "flex:none; height:34px; display:flex; align-items:center; gap:4px; padding:0 12px; border-radius:17px; \
                            border:1px solid {RULE}; background:#1c1e22; color:{ACCENT}; font-family:inherit; font-size:13px; \
                            font-weight:650; cursor:pointer;",
                    onclick: move |_| on_add.call(()),
                    Plus { size: 14, color: "currentColor" }
                    "Add"
                }
            }
        }
    }
}

/// A song's title, and under it its key and writers — and, when it has no
/// session to play yet, that.
#[component]
fn SongWords(song: LibrarySong, playable: bool) -> Element {
    let mut detail: Vec<String> = Vec::new();
    if !song.key.is_empty() {
        detail.push(song.key.clone());
    }
    if !song.writers.is_empty() {
        detail.push(song.writers.join(", "));
    }
    let detail = detail.join(" · ");
    rsx! {
        div {
            style: "flex:1; min-width:0; display:flex; flex-direction:column; gap:2px;",
            span { style: "font-size:15px; font-weight:600; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;", "{song.title}" }
            div {
                style: "display:flex; gap:8px; font-size:12px; color:{DIM}; white-space:nowrap; overflow:hidden;",
                if !playable {
                    span { style: "flex:none; color:{WARN};", "No session yet" }
                }
                span { style: "overflow:hidden; text-overflow:ellipsis;", "{detail}" }
            }
        }
    }
}

/// A square button with an icon, a finger's size.
#[component]
fn IconButton(
    title: String,
    enabled: bool,
    on_press: EventHandler<()>,
    children: Element,
) -> Element {
    let color = if enabled { TEXT } else { "#3a3d44" };
    rsx! {
        button {
            title,
            style: "flex:none; width:40px; height:40px; display:flex; align-items:center; justify-content:center; \
                    border-radius:10px; border:none; background:transparent; color:{color}; cursor:pointer;",
            onclick: move |_| if enabled { on_press.call(()) },
            {children}
        }
    }
}

/// A song of the library, made or changed: its title, key and writers —
/// and, for one that is there already, deleting it. Whole-page, as the
/// list editor is; `song` `None` makes a new one.
#[component]
pub fn SongEditor(
    library: Library,
    song: Option<LibrarySong>,
    /// Done: saved, deleted, or left.
    on_done: EventHandler<()>,
) -> Element {
    let is_new = song.is_none();
    let title = use_signal(|| song.as_ref().map(|s| s.title.clone()).unwrap_or_default());
    let key = use_signal(|| song.as_ref().map(|s| s.key.clone()).unwrap_or_default());
    let writers = use_signal(|| {
        song.as_ref()
            .map(|s| s.writers.join(", "))
            .unwrap_or_default()
    });
    let mut busy = use_signal(|| false);
    let mut deleting = use_signal(|| false);
    let mut problem = use_signal(|| None::<String>);

    let save = {
        let library = library.clone();
        let song = song.clone();
        move |()| {
            if busy() {
                return;
            }
            let name = title().trim().to_owned();
            if name.is_empty() {
                problem.set(Some("A song needs a title.".to_owned()));
                return;
            }
            let list: Vec<String> = writers()
                .split(',')
                .map(str::trim)
                .filter(|w| !w.is_empty())
                .map(str::to_owned)
                .collect();
            let key_now = key().trim().to_owned();
            let library = library.clone();
            let was = song.clone();
            busy.set(true);
            spawn(async move {
                let outcome = off_thread(move || {
                    with_library(&library, |l| async move {
                        match was {
                            Some(was) => {
                                l.update_song(&LibrarySong {
                                    title: name,
                                    key: key_now,
                                    writers: list,
                                    ..was
                                })
                                .await
                            }
                            None => l.create_song(&name, &key_now, &list).await.map(drop),
                        }
                    })
                })
                .await;
                busy.set(false);
                match outcome {
                    Some(Ok(())) => on_done.call(()),
                    Some(Err(e)) => problem.set(Some(brief(&e))),
                    None => {}
                }
            });
        }
    };
    let delete = {
        let library = library.clone();
        let song = song.clone();
        move |()| {
            let Some(song) = song.clone() else { return };
            if !deleting() {
                deleting.set(true);
                return;
            }
            let library = library.clone();
            busy.set(true);
            spawn(async move {
                let slug = song.slug.clone();
                let outcome = off_thread(move || {
                    with_library(&library, |l| async move { l.delete_song(&slug).await })
                })
                .await;
                busy.set(false);
                match outcome {
                    Some(Ok(())) => on_done.call(()),
                    Some(Err(e)) => problem.set(Some(brief(&e))),
                    None => {}
                }
            });
        }
    };

    let field = |label: &'static str, hint: &'static str, value: Signal<String>| {
        let mut value = value;
        rsx! {
            label {
                style: "display:flex; flex-direction:column; gap:6px;",
                span { style: "font-size:12px; font-weight:650; color:{DIM};", "{label}" }
                div {
                    style: "position:relative; height:44px;",
                    if value().is_empty() {
                        span {
                            style: "position:absolute; top:0; left:12px; height:44px; display:flex; align-items:center; \
                                    font-size:15px; color:#6b7280; pointer-events:none;",
                            "{hint}"
                        }
                    }
                    input {
                        style: "position:absolute; top:0; left:0; width:100%; height:44px; box-sizing:border-box; padding:0 12px; \
                                border-radius:10px; border:1px solid {RULE}; background:#101216; color:{TEXT}; \
                                font-family:inherit; font-size:15px;",
                        r#type: "text",
                        value: "{value}",
                        oninput: move |e| value.set(e.value()),
                    }
                }
            }
        }
    };
    let heading = if is_new { "New song" } else { "Song" };
    let save_label = if busy() {
        "Saving…"
    } else if is_new {
        "Make song"
    } else {
        "Save"
    };

    rsx! {
        div {
            style: "position:absolute; inset:0; display:flex; flex-direction:column; background:{BG}; color:{TEXT}; \
                    font-family:system-ui, -apple-system, sans-serif;",
            div {
                style: "flex:none; display:flex; align-items:center; gap:12px; padding:{TOP}px 16px 12px; \
                        border-bottom:1px solid {RULE}; background:{BAR};",
                button {
                    style: "height:36px; display:flex; align-items:center; gap:6px; padding:0 12px 0 8px; border-radius:9px; \
                            border:1px solid {RULE}; background:#1c1e22; color:{TEXT}; font-family:inherit; font-size:14px; cursor:pointer;",
                    onclick: move |_| on_done.call(()),
                    ChevronLeft { size: 18, color: TEXT }
                    "Back"
                }
                span { style: "flex:1; min-width:0; font-size:17px; font-weight:700;", "{heading}" }
                Pill { label: save_label, primary: true, on_press: save }
            }
            div {
                style: "flex:1; min-height:0; overflow-y:auto;",
                div {
                    style: "box-sizing:border-box; width:100%; max-width:560px; padding:20px 20px 40px; \
                            display:flex; flex-direction:column; gap:18px;",
                    {field("Title", "What the song is called", title)}
                    {field("Key", "The key it is usually played in — G, Bb, F#m", key)}
                    {field("Writers", "Who wrote it, separated by commas", writers)}
                    if let Some(why) = problem() {
                        span { style: "font-size:13px; color:{WARN}; line-height:1.45;", "{why}" }
                    }
                    if is_new {
                        span {
                            style: "font-size:13px; color:{DIM}; line-height:1.5;",
                            "A new song can go in setlists straight away. It plays once its session is made — its tracks, chart and lyrics — from REAPER or the desktop app."
                        }
                    } else {
                        div {
                            style: "display:flex; margin-top:10px;",
                            Pill {
                                label: if deleting() { "Tap again to delete this song" } else { "Delete song" },
                                primary: false,
                                on_press: delete,
                            }
                        }
                    }
                }
            }
        }
    }
}
