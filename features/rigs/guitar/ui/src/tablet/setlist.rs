//! The tablet's setlist sidebar — the touch prototype's `setlist/Setlist.tsx`
//! on the live rig.
//!
//!   Header   the set's name (a tap opens the sets), event · date · profile
//!            in words, the set as a bar of song colours; ⋯ manages the set.
//!   Songs    number on the song's colour, name, Now / Next, key, tempo, ⋯.
//!            The song up is open on its sections; a tap on another plays it.
//!            Reorder mode grows handles to drag songs into order.
//!   Sections a part's `section` groups consecutive parts; each shows the
//!            patch it plays, its changes in words, and a ⋯; its grip drags
//!            it. The section playing opens into the stacks.
//!   Stacks   name in its colour, the patch a tap plays, its rotation as
//!            dots (where each patch comes from), what the next tap plays;
//!            ⋯ jumps to a patch, keeps or drops a by-hand pick, makes the
//!            patch the section's, and manages the stack.
//!
//! The pickers (sets, details, add songs, key, tempo, profile, colour, the
//! start, a section's patch) slide over the list — see `panels`.

use std::cell::RefCell;
use std::rc::Rc;

use dioxus::prelude::*;
use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{LibraryModel, PerfPart, PerformanceModel};
use signal_widgets::drag_bus::{DragBus, DragEvent};
use signal_widgets::PopupHost;

use super::colors::{date_label, name_colour, section_colour, set_heading, set_meta, song_colour, suggest_section, tape_for, when_label, TAPE_GAFFER};
use super::marks::{module_colour, OverrideIcon, Strike};
use super::menu::{open_naming_under, Item, MoreButton, Picked, PressMenu};
use super::panels::{Panel, PanelView};
use super::tokens::*;
use crate::state::RigViewState;

/// A rig call from an event handler: clone the client, run it, ignore the
/// reply (the state stream brings the result).
macro_rules! call {
    ($rig:expr, |$r:ident| $body:expr) => {{
        if let Some($r) = $rig.clone() {
            // Forever, not the component's: a panel that closes itself
            // would take the call with it.
            let _ = dioxus_core::spawn_forever(async move {
                let _ = $body.await;
            });
        }
    }};
}
pub(crate) use call;

/// The library, refetched whenever the rig's state moves on.
pub fn use_library(state: RigViewState) -> Signal<LibraryModel> {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut lib = use_signal(LibraryModel::default);
    use_effect(move || {
        let _revision = state.perf.read().revision;
        if let Some(r) = rig.clone() {
            spawn(async move {
                if let Ok(l) = r.library().await {
                    lib.set(l);
                }
            });
        }
    });
    lib
}

/// The stack a patch of `profile` lives in.
pub fn stack_of(lib: &LibraryModel, profile: &str, patch: &str) -> String {
    let in_profile = |p: &signal_guitar_proto::ProfileEntry| p.patch_list.iter().find(|x| x.name.eq_ignore_ascii_case(patch)).map(|x| x.stack.clone());
    lib.profiles
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(profile))
        .and_then(in_profile)
        .or_else(|| lib.profiles.iter().find_map(in_profile))
        .unwrap_or_default()
}

/// The stack a patch plays from (the prototype's `stackOf`): the switch
/// whose rotation holds it, else the stack the library files it under —
/// when that is a stack (a song's own patch files under the song) — else
/// the stack its name says ("Ambient Delay Flute" → Ambient).
pub fn stack_for(perf: &PerformanceModel, lib: &LibraryModel, profile: &str, patch: &str) -> String {
    let real = |st: String| perf.stacks.iter().find(|s| s.name.eq_ignore_ascii_case(&st)).map(|s| s.name.clone());
    perf.stacks
        .iter()
        .find(|s| s.patches.iter().any(|p| p.eq_ignore_ascii_case(patch)))
        .map(|s| s.name.clone())
        .or_else(|| real(stack_of(lib, profile, patch)))
        .or_else(|| {
            let words: Vec<String> = patch.split_whitespace().map(str::to_lowercase).collect();
            perf.stacks.iter().find(|s| words.contains(&s.name.to_lowercase())).map(|s| s.name.clone())
        })
        .unwrap_or_default()
}

/// The profile the rig has active (the one a set without its own plays).
pub fn active_profile(perf: &PerformanceModel, lib: &LibraryModel) -> String {
    lib.profiles.iter().find(|p| p.active).map_or_else(|| perf.profile_name.clone(), |p| p.name.clone())
}

/// A section: consecutive parts that share a `section` name (a part with
/// none is its own).
#[derive(Clone, PartialEq)]
pub struct Section {
    pub name: String,
    /// Indices into `PerformanceModel::parts`.
    pub parts: Vec<usize>,
}

pub fn sections_of(parts: &[PerfPart]) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    for (i, p) in parts.iter().enumerate() {
        let name = if p.section.is_empty() { p.name.clone() } else { p.section.clone() };
        match out.last_mut() {
            Some(last) if !p.section.is_empty() && last.name == name => last.parts.push(i),
            _ => out.push(Section { name, parts: vec![i] }),
        }
    }
    out
}

/// Reorder the song's parts to `want` (a permutation of their indices),
/// one `move_part` at a time, in order — what moving a section is.
pub fn reorder_parts(rig: Option<RigClient>, want: Vec<usize>) {
    let Some(r) = rig else { return };
    let _ = dioxus_core::spawn_forever(async move {
        let mut now: Vec<usize> = (0..want.len()).collect();
        for (pos, &part) in want.iter().enumerate() {
            let Some(at) = now.iter().position(|&p| p == part) else { continue };
            if at != pos {
                let _ = r.move_part(at as u32, pos as u32).await;
                let p = now.remove(at);
                now.insert(pos, p);
            }
        }
    });
}

/// The parts in order with section `from` moved to section position `to`.
fn section_moved(sections: &[Section], from: usize, to: usize) -> Vec<usize> {
    let mut order: Vec<Section> = sections.to_vec();
    if from < order.len() && to < order.len() {
        let s = order.remove(from);
        order.insert(to, s);
    }
    order.into_iter().flat_map(|s| s.parts).collect()
}

/// Rows a list keeps measured handles to, by position.
pub type Rows = Signal<Vec<Option<Rc<MountedData>>>>;

/// Remember row `i`'s element (its `onmounted`).
pub fn keep_row(rows: Rows, i: usize, e: &MountedEvent) {
    let mut rows = rows;
    let mut r = rows.write();
    if r.len() <= i {
        r.resize(i + 1, None);
    }
    r[i] = Some(e.data());
}

/// Drag row `from` by its grip: the rows' tops are measured as it starts,
/// and it lands on the last row whose top (plus `grace`) the pointer is
/// below — the prototype's measured drop.
pub fn begin_row_drag(bus: Option<DragBus>, drag: Signal<Option<(usize, usize)>>, rows: Rows, count: usize, from: usize, grace: f64, on_drop: impl Fn(usize, usize) + 'static) {
    let tops: Rc<RefCell<Vec<f64>>> = Rc::default();
    let els: Vec<Option<Rc<MountedData>>> = rows.peek().iter().take(count).cloned().collect();
    {
        let tops = Rc::clone(&tops);
        spawn(async move {
            let mut out = Vec::with_capacity(els.len());
            for e in els {
                out.push(match e {
                    Some(e) => e.get_client_rect().await.map_or(f64::INFINITY, |r| r.min_y()),
                    None => f64::INFINITY,
                });
            }
            *tops.borrow_mut() = out;
        });
    }
    let mut d = drag;
    d.set(Some((from, from)));
    let Some(bus) = bus else {
        d.set(None);
        return;
    };
    bus.begin(move |ev| {
        let mut d = drag;
        match ev {
            DragEvent::Move { y, .. } => {
                let t = tops.borrow();
                if t.is_empty() {
                    return;
                }
                let mut to = 0;
                for (k, top) in t.iter().enumerate() {
                    if y > top + grace {
                        to = k;
                    }
                }
                d.set(Some((from, to)));
            }
            DragEvent::End => {
                let to = d.peek().map_or(from, |x| x.1);
                d.set(None);
                if to != from {
                    on_drop(from, to);
                }
            }
        }
    });
}

// ── What plays ─────────────────────────────────────────────────────────────

/// Where a patch in a stack comes from: the song put it there, the profile
/// passes it through, or it is borrowed from another profile.
#[derive(Clone, PartialEq, Debug)]
pub enum Src {
    Song,
    Profile(String),
    Other(String),
}

impl Src {
    /// `SourceIcon`'s `from`.
    pub fn from(&self) -> &'static str {
        match self {
            Self::Song => "song",
            Self::Profile(_) => "profile",
            Self::Other(_) => "other",
        }
    }
    pub fn profile(&self) -> Option<String> {
        match self {
            Self::Song => None,
            Self::Profile(p) | Self::Other(p) => Some(p.clone()),
        }
    }
    /// Where it comes from, in words.
    pub fn label(&self) -> String {
        match self {
            Self::Song => "the song's own".to_string(),
            Self::Other(p) => format!("borrowed from {p}"),
            Self::Profile(p) => format!("from {p}, passed through"),
        }
    }
    /// The colour of its source mark.
    pub fn colour(&self, song_colour: &str) -> String {
        match self {
            Self::Song => song_colour.to_string(),
            Self::Other(p) => name_colour(p).to_string(),
            Self::Profile(_) => INK_3.to_string(),
        }
    }
    /// Song's own first, then the profile's, then the borrowed.
    pub fn rank(&self) -> u8 {
        match self {
            Self::Song => 0,
            Self::Profile(_) => 1,
            Self::Other(_) => 2,
        }
    }
}

/// Where `patch` in a stack of `profile` comes from.
pub fn source_of(lib: &LibraryModel, profile: &str, patch: &str) -> Src {
    let own = lib.profiles.iter().find(|p| p.name.eq_ignore_ascii_case(profile));
    if let Some((p, x)) = own.and_then(|p| p.patch_list.iter().find(|x| x.name.eq_ignore_ascii_case(patch)).map(|x| (p, x))) {
        return if x.song.is_empty() { Src::Profile(p.name.clone()) } else { Src::Song };
    }
    lib.profiles
        .iter()
        .find(|p| p.patch_list.iter().any(|x| x.song.is_empty() && x.name.eq_ignore_ascii_case(patch)))
        .map_or(Src::Song, |p| Src::Other(p.name.clone()))
}

/// What a part plays, in words: its patch, `Clean stack`, or a preset's
/// variation; `None` when it keeps what plays.
pub fn sound_of(part: &PerfPart) -> Option<String> {
    if !part.stack.is_empty() {
        Some(format!("{} stack", part.stack))
    } else if !part.preset.is_empty() {
        Some(part.preset.clone())
    } else if !part.patch.is_empty() {
        Some(part.patch.clone())
    } else {
        None
    }
}

/// The patch the part up plays (the prototype's `partPatch`): its own,
/// else the last one before it in the song that sets one (a part that
/// keeps plays on), else the song's start.
pub fn part_patch(perf: &PerformanceModel) -> Option<String> {
    if !perf.parts.is_empty() {
        let at = (perf.part_index as usize).min(perf.parts.len() - 1);
        for p in perf.parts[..=at].iter().rev() {
            if !p.stack.is_empty() {
                // A stack plays its first patch.
                let first = perf.stacks.iter().find(|s| s.name.eq_ignore_ascii_case(&p.stack)).and_then(|s| s.patches.first().cloned());
                return Some(first.unwrap_or_else(|| p.stack.clone()));
            }
            if !p.preset.is_empty() {
                return Some(p.preset.clone());
            }
            if !p.patch.is_empty() {
                return Some(p.patch.clone());
            }
        }
    }
    let start = perf.songs.get(perf.song_index as usize).map(|s| s.start.clone()).unwrap_or_default();
    (!start.is_empty()).then_some(start)
}

/// The patch playing: the active switch's.
fn playing(perf: &PerformanceModel) -> Option<String> {
    perf.stacks.iter().find(|s| s.is_active).map(|s| s.patches.get(s.position as usize).cloned().unwrap_or_else(|| s.current_patch.clone()))
}

/// Something other than the part's patch plays — picked by hand.
pub fn is_live(perf: &PerformanceModel, own: Option<&str>) -> bool {
    matches!((own, playing(perf)), (Some(o), Some(p)) if !o.eq_ignore_ascii_case(&p))
}

/// The section (or, in one of several parts, the part) playing, by name:
/// what "Keep for …" and "Make … default for …" speak of.
pub fn where_label(perf: &PerformanceModel) -> String {
    let now = perf.part_index as usize;
    let song = perf.songs.get(perf.song_index as usize).map(|s| s.name.clone());
    match sections_of(&perf.parts).into_iter().find(|s| s.parts.contains(&now)) {
        None => song.unwrap_or_else(|| "this song".to_string()),
        Some(sec) if sec.parts.len() > 1 => perf.parts.get(now).map_or(sec.name.clone(), |p| p.name.clone()),
        Some(sec) => sec.name,
    }
}

/// One stack as a list shows it: its patches and where each comes from,
/// whether it plays, and where its rotation is.
#[derive(Clone, PartialEq, Debug)]
pub struct StackView {
    /// Its index in the profile's stacks (what `press_stack` takes).
    pub index: usize,
    pub name: String,
    pub patches: Vec<(String, Src)>,
    /// Each patch's place in the rig's rotation (what `play_stack_patch`
    /// takes).
    pub at: Vec<usize>,
    pub on: bool,
    /// The rotation's place among `patches`.
    pub cursor: usize,
}

/// The song's stacks: every stack the profile fills, with what the song
/// put in them.
pub fn song_stacks(perf: &PerformanceModel, lib: &LibraryModel) -> Vec<StackView> {
    perf.stacks
        .iter()
        .enumerate()
        .filter(|(_, s)| !s.patches.is_empty())
        .map(|(i, s)| StackView {
            index: i,
            name: s.name.clone(),
            patches: s.patches.iter().map(|p| (p.clone(), source_of(lib, &perf.profile_name, p))).collect(),
            at: (0..s.patches.len()).collect(),
            on: s.is_active,
            cursor: s.position as usize % s.patches.len(),
        })
        .collect()
}

/// The profile's stacks on their own — no song's patches in them.
pub fn profile_stacks(perf: &PerformanceModel, lib: &LibraryModel) -> Vec<StackView> {
    song_stacks(perf, lib)
        .into_iter()
        .zip(perf.stacks.iter().filter(|s| !s.patches.is_empty()))
        .filter_map(|(v, s)| {
            let at = s.patches.get(s.position as usize).cloned().unwrap_or_default();
            let (patches, at_pos): (Vec<(String, Src)>, Vec<usize>) = v.patches.into_iter().zip(v.at).filter(|((_, src), _)| *src != Src::Song).unzip();
            if patches.is_empty() {
                return None;
            }
            let found = patches.iter().position(|(p, _)| p.eq_ignore_ascii_case(&at));
            Some(StackView { on: v.on && found.is_some(), cursor: found.unwrap_or(0), patches, at: at_pos, ..v })
        })
        .collect()
}

/// Give the part up `patch` as its own — keeping where it is borrowed
/// from, or dropping a borrowed profile it no longer needs.
pub fn give_part(rig: Option<RigClient>, part: &PerfPart, patch: String, borrowed: Option<String>) {
    let Some(r) = rig else { return };
    let (name, had) = (part.name.clone(), !part.profile.is_empty());
    let _ = dioxus_core::spawn_forever(async move {
        let _ = r.set_part_patch(name.clone(), patch).await;
        match borrowed {
            Some(p) => {
                let _ = r.set_part_profile(name, p).await;
            }
            None if had => {
                let _ = r.set_part_profile(name, String::new()).await;
            }
            None => {}
        }
    });
}

/// Play `patch` by name: the rig's patch list says its index.
pub fn play_patch(rig: Option<RigClient>, patch: String) {
    let Some(r) = rig else { return };
    let _ = dioxus_core::spawn_forever(async move {
        if let Ok(list) = r.patches().await
            && let Some(i) = list.iter().position(|p| p.name.eq_ignore_ascii_case(&patch))
        {
            let _ = r.select_patch(i as u32).await;
        }
    });
}

/// The modules some parts pick of their own, once each.
fn part_modules(all: &[PerfPart], parts: &[usize]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for &k in parts {
        let Some(p) = all.get(k) else { continue };
        for pk in &p.picks {
            let kind = pk.kind.strip_prefix("block:").unwrap_or(&pk.kind).to_string();
            if !out.contains(&kind) {
                out.push(kind);
            }
        }
        if p.picks.is_empty() {
            for o in &p.overrides {
                if !out.iter().any(|b| b.eq_ignore_ascii_case(&o.block)) {
                    out.push(o.block.clone());
                }
            }
        }
    }
    out
}

/// Song `i` of the set's parts: the live ones for the song up, else the
/// library's.
fn song_parts(perf: &PerformanceModel, lib: &LibraryModel, i: usize) -> Vec<PerfPart> {
    if i == perf.song_index as usize {
        return perf.parts.clone();
    }
    let name = perf.songs.get(i).map(|s| s.name.clone()).unwrap_or_default();
    lib.songs.iter().find(|s| s.name.eq_ignore_ascii_case(&name)).map(|s| s.part_list.clone()).unwrap_or_default()
}

// ── The sidebar ────────────────────────────────────────────────────────────

#[component]
pub fn TabletSetlist(state: RigViewState) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let bus = DragBus::try_use();
    let lib = use_library(state);
    let perf = state.perf.read().clone();
    let lib_now = lib.read().clone();
    let songs = perf.songs.clone();
    let mut panel = use_signal(|| None::<Panel>);
    let reordering = use_signal(|| false);
    // Songs opened on their sections, beside the one up (always open).
    let mut open = use_signal(std::collections::BTreeSet::<usize>::new);
    let at = perf.song_index as usize;
    let set_index = perf.setlist_index;
    // A song being dragged into order: (from, to).
    let drag = use_signal(|| None::<(usize, usize)>);
    // The list and its songs, measured: whether the song up is scrolled
    // out of the list, and which way.
    let mut list_el = use_signal(|| None::<Rc<MountedData>>);
    let rows: Rows = use_signal(Vec::new);
    let mut away = use_signal(|| None::<bool>);
    let perf_sig = state.perf;
    let measure = move || {
        let at = perf_sig.peek().song_index as usize;
        let up = rows.peek().get(at).cloned().flatten();
        let (Some(list), Some(up)) = (list_el.peek().clone(), up) else { return };
        spawn(async move {
            // Mounted before laid out: wait for a layout to measure.
            for _ in 0..10 {
                if let (Ok(l), Ok(u)) = (list.get_client_rect().await, up.get_client_rect().await)
                    && l.height() > 1.0
                    && u.height() > 1.0
                {
                    break;
                }
                architect::platform::sleep(std::time::Duration::from_millis(50)).await;
            }
            let (Ok(l), Ok(u)) = (list.get_client_rect().await, up.get_client_rect().await) else { return };
            if l.height() <= 1.0 || u.height() <= 1.0 {
                return;
            }
            let now = if u.max_y() < l.min_y() + 1.0 {
                Some(true)
            } else if u.min_y() > l.max_y() - 1.0 {
                Some(false)
            } else {
                None
            };
            if *away.peek() != now {
                away.set(now);
            }
        });
    };
    // The song up moves on: measure where the new one is.
    use_effect(move || {
        let _song = perf_sig.read().song_index;
        measure();
    });
    let count = songs.len();
    rsx! {
        section { style: "position: relative; height: 100%; display: flex; flex-direction: column; min-height: 0; overflow: hidden; background: {SHEET}; font-family: {FONT}; color: {INK};",
            SetHeader { perf: perf.clone(), lib: lib_now.clone(), panel, reordering }
            div { style: "position: relative; flex: 1; min-height: 0; display: flex; flex-direction: column;",
            div {
                style: "position: relative; flex: 1; min-height: 0; overflow-y: auto;",
                onmounted: move |e| list_el.set(Some(e.data())),
                onscroll: move |_| measure(),
                for (i, song) in songs.iter().enumerate() {
                    div {
                        key: "{i}-{song.name}",
                        onmounted: move |e| {
                            keep_row(rows, i, &e);
                            if i == perf_sig.peek().song_index as usize {
                                measure();
                            }
                        },
                        SongRow {
                            perf: perf.clone(),
                            lib: lib_now.clone(),
                            index: i,
                            open: !reordering() && (i == at || open.read().contains(&i)),
                            reordering: reordering(),
                            drag,
                            panel,
                            on_toggle: move |()| {
                                let mut o = open.write();
                                if !o.remove(&i) {
                                    o.insert(i);
                                }
                            },
                            on_grip: {
                                let rig = rig.clone();
                                move |_: PointerEvent| {
                                    let rig = rig.clone();
                                    begin_row_drag(bus, drag, rows, count, i, 24.0, move |from, to| {
                                        call!(rig, |r| r.move_setlist_entry(set_index, from as u32, to as u32));
                                    });
                                }
                            },
                        }
                        // The song up is open on its sections; others when opened.
                        if !reordering() && (i == at || open.read().contains(&i)) {
                            Sections { perf: perf.clone(), lib: lib_now.clone(), song: i, panel }
                        }
                    }
                }
                if !songs.is_empty() && !reordering() {
                    button {
                        style: "display: flex; align-items: center; gap: 10px; width: 100%; min-height: 52px; padding: 0 18px; border: none; border-top: 1px solid {RULE}; background: transparent; color: {INK_3}; font-size: 15px; font-weight: 600; font-family: {FONT}; text-align: left; justify-content: flex-start; cursor: pointer;",
                        onclick: move |_| panel.set(Some(Panel::Add)),
                        Plus { size: 14 }
                        "Add songs"
                    }
                }
                if songs.is_empty() {
                    div { style: "padding: 28px 20px; display: flex; flex-direction: column; align-items: flex-start; gap: 10px;",
                        div { style: "font-size: 18px; font-weight: 700;", "No songs yet" }
                        PrimaryButton { label: "Add songs", onclick: move |()| panel.set(Some(Panel::Add)) }
                    }
                }
            }
            // Scrolled away from the song up: where it is, and a way back.
            if let Some(above) = away() {
                NowBar { perf: perf.clone(), above, on_back: move |()| {
                    let at = perf_sig.peek().song_index as usize;
                    if let Some(up) = rows.peek().get(at).cloned().flatten() {
                        spawn(async move {
                            let _ = up.scroll_to(ScrollBehavior::Smooth).await;
                        });
                    }
                } }
            }
            }
            if let Some(p) = panel() {
                PanelView { panel: p, perf: perf.clone(), lib: lib_now.clone(), on_close: move |()| panel.set(None) }
            }
        }
    }
}

/// Pinned over the list while the song up is scrolled out of it: the song
/// and the section playing, and a tap back to them.
#[component]
fn NowBar(perf: PerformanceModel, above: bool, on_back: EventHandler<()>) -> Element {
    let Some(song) = perf.songs.get(perf.song_index as usize).cloned() else { return rsx! {} };
    let colour = song_colour(&song.name, &song.colour);
    let section = perf.parts.get(perf.part_index as usize).map(|p| if p.section.is_empty() { p.name.clone() } else { p.section.clone() }).unwrap_or_default();
    let edge = if above { "top: 0px; box-shadow: 0 6px 16px rgba(0,0,0,0.45);" } else { "bottom: 0px; box-shadow: 0 -6px 16px rgba(0,0,0,0.45);" };
    rsx! {
        button {
            style: "position: absolute; left: 0px; right: 0px; {edge} z-index: 5; height: 44px; display: flex; align-items: center; justify-content: flex-start; gap: 10px; padding: 0 14px; border: none; text-align: left; background: {tint(&colour, 16)}; color: {INK}; font-family: {FONT}; cursor: pointer;",
            onclick: move |_| on_back.call(()),
            span { style: "width: 22px; height: 22px; border-radius: 5px; flex-shrink: 0; display: flex; align-items: center; justify-content: center; font-size: 12px; font-weight: 750; background: {colour}; color: #0b0b0e;", "{perf.song_index + 1}" }
            span { style: "font-size: 15px; font-weight: 700; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{song.name}" }
            if !section.is_empty() {
                span { style: "font-size: 14px; font-weight: 650; color: {section_colour(&section)}; white-space: nowrap;", "{section}" }
            }
            span { style: "flex: 1;" }
            span { style: "display: flex; align-items: center; gap: 5px; font-size: 12px; font-weight: 700; color: {INK_2}; white-space: nowrap;",
                svg { key: "{above}", width: "10", height: "12", view_box: "0 0 10 12",
                    path { d: pick(above, "M5 11V1.5M1 5.5 5 1.5l4 4", "M5 1V10.5M1 6.5 5 10.5l4-4"), fill: "none", stroke: INK_2, stroke_width: "1.7", stroke_linecap: "round", stroke_linejoin: "round" }
                }
                "Now"
            }
        }
    }
}

#[component]
pub fn Plus(size: u32) -> Element {
    rsx! {
        svg { width: "{size}", height: "{size}", view_box: "0 0 12 12", style: "flex-shrink: 0;",
            path { d: "M6 1v10M1 6h10", stroke: INK_3, stroke_width: "1.6", stroke_linecap: "round" }
        }
    }
}

/// The primary button: the one thing to do here (the kit's `Button
/// primary`).
#[component]
pub fn PrimaryButton(label: String, onclick: EventHandler<()>, disabled: Option<bool>) -> Element {
    let off = disabled.unwrap_or(false);
    rsx! {
        button {
            disabled: off,
            style: "min-height: {HIT}px; padding: 0 18px; border-radius: {R}; border: 1px solid {pick(off, RULE_STRONG, DIM)}; background: {pick(off, CLEAR, DIM)}; color: {pick(off, INK_3, \"#ffffff\")}; font-size: 15px; font-weight: 650; font-family: {FONT}; white-space: nowrap; cursor: pointer;",
            onclick: move |_| if !off { onclick.call(()) },
            "{label}"
        }
    }
}

// ── Header ─────────────────────────────────────────────────────────────────

#[component]
fn SetHeader(perf: PerformanceModel, lib: LibraryModel, panel: Signal<Option<Panel>>, reordering: Signal<bool>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let set_index = perf.setlist_index as usize;
    let entry = lib.setlists.get(set_index).cloned().unwrap_or_default();
    let name = perf.setlists.get(set_index).cloned().unwrap_or_else(|| entry.name.clone());
    let meta = set_meta(&name, &entry.event, &entry.date, &entry.title);
    let heading = set_heading(&meta);
    let date = date_label(&meta.date);
    let when = when_label(&meta.date);
    // Only today and tomorrow are worth a word beside the date.
    let soon = (when == "Today" || when == "Tomorrow").then(|| when.clone());
    // The set's profile, else the rig's.
    let profile = if entry.profile.is_empty() { active_profile(&perf, &lib) } else { entry.profile.clone() };
    let count = perf.songs.len();
    let at = perf.song_index as usize;
    let sets = perf.setlists.len();

    let actions = vec![
        Item::head(name.clone()),
        Item::run("add", "Add songs…"),
        Item::run("edit", "Edit details…").detail("event · date · title"),
        Item::run("profile", "Default profile…").detail(if entry.profile.is_empty() { "the rig's".to_string() } else { entry.profile.clone() }),
        Item::run("new", "New set…"),
        Item::run("duplicate", "Duplicate for next week"),
        Item::run("sets", "All sets…").detail(format!("{sets}")),
        Item::run("reorder", "Reorder songs").unless((count < 2).then(|| "Fewer than two songs".to_string())),
        Item::Sep,
        Item::delete("delete", "Delete set").unless((sets <= 1).then(|| "The only set — make another first".to_string())),
    ];
    let mut panel_w = panel;
    let mut reorder_w = reordering;

    rsx! {
        header { style: "flex-shrink: 0; height: {HEADER_H}px; padding: 0 6px 0 18px; border-bottom: 1px solid {RULE}; display: flex; flex-direction: column; justify-content: center; gap: 7px; box-sizing: border-box;",
            div { style: "display: flex; align-items: center; gap: 2px;",
                // The set's name and a chevron: a tap opens the sets.
                button {
                    style: "flex: 1; min-width: 0; min-height: 44px; display: flex; flex-direction: column; justify-content: center; align-items: flex-start; gap: 3px; text-align: left; padding: 0 8px; margin: 0 0 0 -8px; border: none; border-radius: {R_MD}; background: transparent; color: {INK}; font-family: {FONT}; cursor: pointer;",
                    onclick: move |_| panel_w.set(Some(Panel::Sets)),
                    span { style: "display: flex; align-items: center; gap: 8px; min-width: 0; max-width: 100%;",
                        span { style: "min-width: 0; font-size: 21px; font-weight: 750; letter-spacing: -0.02em; line-height: 1.1; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{heading}" }
                        svg { width: "10", height: "6", view_box: "0 0 10 6", style: "flex-shrink: 0;",
                            path { d: "M1 1 L5 5 L9 1", fill: "none", stroke: INK_3, stroke_width: "1.8", stroke_linecap: "round", stroke_linejoin: "round" }
                        }
                    }
                    // Event · date · profile — words, no chips.
                    div { style: "display: flex; align-items: center; gap: 6px; max-width: 100%; font-size: 13px; font-weight: 560; color: {INK_3}; white-space: nowrap; overflow: hidden;",
                        if !meta.title.is_empty() && !meta.event.is_empty() {
                            span { style: "width: 7px; height: 7px; border-radius: 999px; flex-shrink: 0; background: {name_colour(&meta.event)};" }
                            span { style: "color: {INK_2}; font-weight: 650;", "{meta.event}" }
                            span { "·" }
                        }
                        span { "{date}" }
                        if let Some(w) = soon.clone() {
                            span { style: "color: {pick(w == \"Today\", LIVE, INK_2)}; font-weight: 650;", "{w}" }
                        }
                        span { "·" }
                        span { style: "overflow: hidden; text-overflow: ellipsis;", "{profile}" }
                    }
                }
                if reordering() {
                    PrimaryButton { label: "Done", onclick: move |()| reorder_w.set(false) }
                    span { style: "width: 6px;" }
                } else {
                    MoreButton {
                        items: actions,
                        label: "Set actions".to_string(),
                        on_pick: {
                            let rig = rig.clone();
                            move |p: Picked| match p.id.as_str() {
                                "add" => panel_w.set(Some(Panel::Add)),
                                "edit" => panel_w.set(Some(Panel::Details(super::panels::DetailsMode::Edit))),
                                "profile" => panel_w.set(Some(Panel::Profile(None))),
                                "new" => panel_w.set(Some(Panel::Details(super::panels::DetailsMode::New))),
                                "duplicate" => panel_w.set(Some(Panel::Details(super::panels::DetailsMode::Duplicate))),
                                "sets" => panel_w.set(Some(Panel::Sets)),
                                "reorder" => reorder_w.set(true),
                                "delete" => call!(rig, |r| r.delete_setlist(set_index as u32)),
                                _ => {}
                            }
                        },
                    }
                }
            }
            if count > 0 {
                // The set as a bar: a segment per song in its colour — played
                // ones dim, the one up full and taller — and its place.
                div { style: "display: flex; align-items: center; gap: 10px; padding-right: 12px;",
                    div { style: "flex: 1; min-width: 0; display: flex; align-items: center; gap: 3px; height: 10px;",
                        for (i, s) in perf.songs.iter().enumerate() {
                            span {
                                key: "{i}",
                                style: "flex: 1 1 0; height: {pick(i == at, 8, 4)}px; border-radius: 2px; background: {song_colour(&s.name, &s.colour)}; opacity: {pick(i < at, 0.3, pick(i == at, 1.0, 0.6))};",
                            }
                        }
                    }
                    span { style: "flex-shrink: 0; font-size: 13px; font-weight: 650; color: {INK_2}; font-variant-numeric: tabular-nums;",
                        "{at + 1}"
                        span { style: "color: {INK_3}; font-weight: 560;", " / {count}" }
                    }
                }
            }
        }
    }
}

// ── A song ─────────────────────────────────────────────────────────────────

/// The song up, pinned to the top of the list while its sections scroll.
const STICK_SONG: &str = "position: sticky; top: 0px; z-index: 3;";
/// The section playing, pinned under the song while its stacks scroll.
const STICK_SECTION: &str = "position: sticky; top: 64px; z-index: 2;";

const UNSTUCK: &str = "position: relative;";

#[component]
fn SongRow(
    perf: PerformanceModel,
    lib: LibraryModel,
    index: usize,
    open: bool,
    reordering: bool,
    drag: Signal<Option<(usize, usize)>>,
    panel: Signal<Option<Panel>>,
    on_toggle: EventHandler<()>,
    on_grip: EventHandler<PointerEvent>,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let Some(song) = perf.songs.get(index).cloned() else { return rsx! {} };
    let at = perf.song_index as usize;
    let up = index == at;
    let played = index < at;
    let next = index == at + 1;
    let count = perf.songs.len();
    let last = count.saturating_sub(1);
    let colour = song_colour(&song.name, &song.colour);
    let lib_song = lib.songs.iter().find(|s| s.name.eq_ignore_ascii_case(&song.name)).cloned();
    let parts = song_parts(&perf, &lib, index);
    let sections = sections_of(&parts).len();
    let own_profile = lib_song.as_ref().map(|s| s.profile.clone()).unwrap_or_default();
    let set_profile = lib.setlists.get(perf.setlist_index as usize).map(|s| s.profile.clone()).unwrap_or_default();
    let shown_profile = if !own_profile.is_empty() {
        own_profile.clone()
    } else if !set_profile.is_empty() {
        set_profile.clone()
    } else {
        active_profile(&perf, &lib)
    };
    let start = song.start.clone();
    let start_stack = stack_for(&perf, &lib, &shown_profile, &start);
    let set_index = perf.setlist_index;
    let (lifted, drop_above) = match drag() {
        Some((from, to)) => (from == index, to == index && from != index),
        None => (false, false),
    };

    let items = vec![
        Item::head(format!("{} · {}", index + 1, song.name)),
        Item::run("go", "Play from here").unless(up.then(|| "It's up now".to_string())),
        Item::Sep,
        Item::run("key", "Key…").detail(if song.key.is_empty() { "—".to_string() } else { song.key.clone() }),
        Item::run("tempo", "Tempo…").detail(if song.bpm > 0 { format!("{} BPM", song.bpm) } else { "—".to_string() }),
        Item::run("start", "Starts on…").detail(if start.is_empty() { "default".to_string() } else { start.clone() }),
        Item::run("profile", "Profile…").detail(if own_profile.is_empty() { format!("{shown_profile} (set's)") } else { own_profile.clone() }),
        Item::run("colour", "Colour…").detail(if song.colour.is_empty() { "from its name" } else { "set" }),
        Item::Sep,
        Item::run("up", "Move up").unless((index == 0).then(|| "Already first".to_string())),
        Item::run("down", "Move down").unless((index == last).then(|| "Already last".to_string())),
        Item::Sep,
        Item::delete("remove", "Remove from this set"),
    ];
    // Its own changes over its patches: the modules its sections pick.
    let modules = part_modules(&parts, &(0..parts.len()).collect::<Vec<_>>());
    let mut panel_w = panel;
    let bg = if lifted {
        SHEET_2.to_string()
    } else if open {
        tint(&colour, pick(up, 7, 4))
    } else {
        CLEAR.to_string()
    };
    // The song up stays at the top while you scroll through it.
    let pin = if up && !reordering { STICK_SONG } else { UNSTUCK };
    let top_rule = if drop_above { format!("border-top: 3px solid {FOCUS_FG};") } else { format!("border-top: 1px solid {RULE};") };
    let number_bg = if played { format!("color-mix(in srgb, {colour} 35%, {SHEET})") } else { colour.clone() };

    let pick_handler = EventHandler::new({
        let rig = rig.clone();
        move |p: Picked| match p.id.as_str() {
            "go" => call!(rig, |r| r.select_song(index as u32)),
            "key" => panel_w.set(Some(Panel::Key(index))),
            "tempo" => panel_w.set(Some(Panel::Tempo(index))),
            "start" => panel_w.set(Some(Panel::Start(index))),
            "profile" => panel_w.set(Some(Panel::Profile(Some(index)))),
            "colour" => panel_w.set(Some(Panel::Colour(index))),
            "up" => call!(rig, |r| r.move_setlist_entry(set_index, index as u32, index as u32 - 1)),
            "down" => call!(rig, |r| r.move_setlist_entry(set_index, index as u32, index as u32 + 1)),
            "remove" => call!(rig, |r| r.remove_setlist_entry(set_index, index as u32)),
            _ => {}
        }
    });
    rsx! {
        PressMenu {
            items: items.clone(),
            on_pick: pick_handler,
            off: reordering,
            style: "{pin} display: flex; align-items: stretch; min-height: 64px; {top_rule} background: {bg};",
            if up {
                span { style: "position: absolute; left: 0; top: 0; bottom: 0; width: 3px; background: {colour};" }
            }
            button {
                style: "flex: 1; min-width: 0; display: flex; align-items: center; justify-content: flex-start; gap: 10px; padding: 10px 2px 10px 14px; text-align: left; background: transparent; border: none; color: inherit; font: inherit; cursor: pointer;",
                onclick: {
                    let rig = rig.clone();
                    move |_| {
                        // The song up stays open (a tap keeps it open after it);
                        // another is played.
                        if reordering {
                        } else if up {
                            on_toggle.call(());
                        } else {
                            call!(rig, |r| r.select_song(index as u32));
                        }
                    }
                },
                span { style: "width: 28px; height: 28px; border-radius: 7px; flex-shrink: 0; display: flex; align-items: center; justify-content: center; font-size: 14px; font-weight: 750; color: #0b0b0e; background: {number_bg};",
                    "{index + 1}"
                }
                span { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px;",
                    span { style: "display: flex; align-items: center; gap: 8px; min-width: 0;",
                        span { style: "position: relative; min-width: 0; font-size: 17px; font-weight: {pick(up, 750, 600)}; color: {pick(played, INK_3, INK)}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                            "{song.name}"
                            if played {
                                Strike { width: 1.6 }
                            }
                        }
                        if up { Badge { live: true, "Now" } }
                        if next { Badge { live: false, "Next" } }
                    }
                    span { style: "display: flex; align-items: center; gap: 6px; font-size: 13px; font-weight: 500; color: {INK_3}; min-width: 0; white-space: nowrap; overflow: hidden;",
                        if !start.is_empty() {
                            PatchChip { patch: start.clone(), stack: start_stack.clone(), small: true, lit: false }
                        }
                        // Its own profile, in words — only when it isn't the set's.
                        if !own_profile.is_empty() {
                            span { style: "flex-shrink: 0; color: {INK_2}; font-weight: 650;", "{own_profile}" }
                        }
                        Changes { modules: modules.clone(), max: 2 }
                        if !open && sections > 0 {
                            span { style: "flex-shrink: 0;", "{sections} sections" }
                        }
                    }
                }
                if !song.key.is_empty() {
                    span { style: "flex-shrink: 0; min-width: 30px; height: 28px; padding: 0 6px; box-sizing: border-box; border: 1px solid {RULE_STRONG}; border-radius: 4px; display: flex; align-items: center; justify-content: center; font-size: 14px; font-weight: 650; color: {INK_2}; font-variant-numeric: tabular-nums;",
                        "{song.key}"
                    }
                }
                span { style: "width: 30px; text-align: right; font-size: 14px; font-weight: 600; color: {INK_2}; flex-shrink: 0; font-variant-numeric: tabular-nums;",
                    if song.bpm > 0 { "{song.bpm}" }
                }
            }
            div { style: "display: flex; align-items: center;",
                if reordering {
                    // The handle: drag the song into its place.
                    span {
                        "aria-label": "Drag {song.name}",
                        style: "width: 52px; height: 52px; display: flex; align-items: center; justify-content: center; cursor: grab; touch-action: none;",
                        onpointerdown: move |e: PointerEvent| {
                            e.prevent_default();
                            e.stop_propagation();
                            on_grip.call(e);
                        },
                        svg { width: "14", height: "10", view_box: "0 0 14 10",
                            path { d: "M1 1h12M1 5h12M1 9h12", stroke: INK_2, stroke_width: "1.8", stroke_linecap: "round" }
                        }
                    }
                } else {
                    MoreButton {
                        items: items.clone(),
                        label: format!("{} actions", song.name),
                        on_pick: pick_handler,
                    }
                }
            }
        }
    }
}

/// Its own changes, in words: the override icon, then each module it
/// picks by name in that module's colour. A song shows a few, then a count.
#[component]
fn Changes(modules: Vec<String>, max: usize) -> Element {
    if modules.is_empty() {
        return rsx! {};
    }
    let shown: Vec<String> = modules.iter().take(max).cloned().collect();
    let more = modules.len().saturating_sub(shown.len());
    let n = shown.len();
    rsx! {
        span { style: "display: inline-flex; align-items: center; gap: 6px; min-width: 0; font-size: 12px; font-weight: 650; white-space: nowrap; overflow: hidden;",
            OverrideIcon { colour: INK_2.to_string(), size: 11 }
            for (k, m) in shown.into_iter().enumerate() {
                span { key: "{m}", style: "color: {lift(module_colour(&m))};",
                    "{m}"
                    if k + 1 < n { span { style: "color: {INK_3};", " ·" } }
                }
            }
            if more > 0 { span { style: "color: {INK_3};", "+{more}" } }
        }
    }
}

#[component]
fn Badge(live: bool, children: Element) -> Element {
    let (bg, fg) = if live { (LIVE_BG, LIVE) } else { ("rgba(255,255,255,0.07)", INK_2) };
    rsx! {
        span { style: "flex-shrink: 0; font-size: 11px; font-weight: 700; letter-spacing: 0.1em; text-transform: uppercase; padding: 2px 6px; border-radius: 4px; background: {bg}; color: {fg};",
            {children}
        }
    }
}

/// A patch with the stack it lands on: the stack named in its colour; a
/// patch borrowed from another profile says from where.
#[component]
pub fn PatchChip(patch: String, stack: String, small: bool, lit: bool, borrowed: Option<String>) -> Element {
    let tape = tape_for(&stack);
    let stack_ink = if tape == TAPE_GAFFER { INK_3.to_string() } else { format!("color-mix(in oklab, {tape} 80%, white)") };
    let (pad, size, stack_size) = if small { ("2px 8px 2px 7px", 12, 10.5) } else { ("5px 9px 5px 8px", 13, 11.0) };
    rsx! {
        span { style: "display: inline-flex; align-items: center; gap: 7px; min-width: 0; max-width: 100%; padding: {pad}; border-radius: 4px; background: {pick(lit, FILL_ON, FILL)}; color: {pick(lit, INK, INK_2)}; font-size: {size}px; font-weight: 600; white-space: nowrap; box-sizing: border-box;",
            if !stack.is_empty() {
                span { style: "flex-shrink: 0; font-size: {stack_size}px; font-weight: 750; letter-spacing: 0.07em; text-transform: uppercase; color: {stack_ink};", "{stack}" }
            }
            span { style: "overflow: hidden; text-overflow: ellipsis;", "{patch}" }
            if let Some(b) = borrowed {
                span { style: "flex-shrink: 0; color: {name_colour(&b)}; font-weight: 650;", "{b}" }
            }
        }
    }
}

// ── Its sections ───────────────────────────────────────────────────────────

#[component]
fn Sections(perf: PerformanceModel, lib: LibraryModel, song: usize, panel: Signal<Option<Panel>>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let host = PopupHost::try_use();
    let bus = DragBus::try_use();
    let parts = song_parts(&perf, &lib, song);
    let sections = sections_of(&parts);
    let up = song == perf.song_index as usize;
    let colour = perf.songs.get(song).map(|s| song_colour(&s.name, &s.colour)).unwrap_or_default();
    let now = if up { perf.part_index as usize } else { usize::MAX };
    let drag = use_signal(|| None::<(usize, usize)>);
    let rows: Rows = use_signal(Vec::new);
    let mut add_el = use_signal(|| None::<Rc<MountedData>>);
    let names: Vec<String> = sections.iter().map(|s| s.name.clone()).collect();
    let count = sections.len();
    let song_name = perf.songs.get(song).map(|s| s.name.clone()).unwrap_or_default();
    rsx! {
        div { style: "position: relative; padding: 2px 0 10px; background: {tint(&colour, pick(up, 7, 4))};",
            if up {
                span { style: "position: absolute; left: 0; top: 0; bottom: 0; width: 3px; background: {colour};" }
            }
            // One box per section: its pinned row stays inside it, so the
            // row lets go once its stacks have scrolled past.
            for (j, sec) in sections.iter().enumerate() {
                {
                    let (lifted, line) = match drag() {
                        Some((from, to)) if to == j && from != j => (false, Some(to < from)),
                        Some((from, _)) => (from == j, None),
                        None => (false, None),
                    };
                    let rig = rig.clone();
                    let order = sections.clone();
                    rsx! {
                        div {
                            key: "{j}-{sec.name}",
                            style: "position: relative; opacity: {pick(lifted, 0.5, 1.0)};",
                            onmounted: move |e| keep_row(rows, j, &e),
                            SectionRow {
                                perf: perf.clone(),
                                lib: lib.clone(),
                                parts: parts.clone(),
                                song,
                                section: sec.clone(),
                                index: j,
                                count,
                                names: names.clone(),
                                panel,
                                on_grip: {
                                    let song_name = song_name.clone();
                                    move |_: PointerEvent| {
                                        let (rig, order, song_name) = (rig.clone(), order.clone(), song_name.clone());
                                        begin_row_drag(bus, drag, rows, count, j, 20.0, move |from, to| {
                                            if up {
                                                reorder_parts(rig.clone(), section_moved(&order, from, to));
                                            } else {
                                                reorder_song_parts(rig.clone(), song_name.clone(), section_moved(&order, from, to));
                                            }
                                        });
                                    }
                                },
                            }
                            // The section playing opens into the stacks.
                            if sec.parts.contains(&now) {
                                Stacks { perf: perf.clone(), lib: lib.clone(), left: 46 }
                            }
                            // Where the dragged section lands: a line on the
                            // side it comes from.
                            if let Some(above) = line {
                                span { style: "position: absolute; left: 0; right: 0; {pick(above, \"top\", \"bottom\")}: 0px; height: 2px; background: {INK_2}; z-index: 4; pointer-events: none;" }
                            }
                        }
                    }
                }
            }
            // No sections: the song itself is what plays, so its stacks sit
            // under it.
            if up && sections.is_empty() {
                Stacks { perf: perf.clone(), lib: lib.clone(), left: 34 }
            }
            // Any song's sections can be added to.
            if !song_name.is_empty() {
            button {
                style: "display: flex; align-items: center; gap: 10px; width: 100%; min-height: 44px; padding: 0 14px 0 28px; border: none; background: transparent; color: {INK_2}; font-size: 14px; font-weight: 600; font-family: {FONT}; text-align: left; justify-content: flex-start; cursor: pointer;",
                onmounted: move |e| add_el.set(Some(e.data())),
                onclick: move |_| {
                    let (rig, song_name) = (rig.clone(), song_name.clone());
                    open_naming_under(host, add_el.peek().clone(), Item::name("add", "New section…", suggest_section(&names), "Add", names.clone()), EventHandler::new(move |p: Picked| {
                        let name = p.text.clone();
                        if up {
                            call!(rig, |r| r.add_part(name));
                        } else {
                            let song = song_name.clone();
                            call!(rig, |r| r.edit_song_part(song, "add".to_string(), String::new(), name));
                        }
                    }));
                },
                span { style: "width: 16px; height: 16px; border-radius: 999px; border: 1px dashed {INK_3}; box-sizing: border-box; display: flex; align-items: center; justify-content: center;",
                    svg { width: "8", height: "8", view_box: "0 0 8 8",
                        path { d: "M4 0.5v7M0.5 4h7", stroke: INK_2, stroke_width: "1.5", stroke_linecap: "round" }
                    }
                }
                "Add a section"
            }
            }
        }
    }
}

/// Reorder library song `song`'s parts to `want`, one move at a time (a
/// song that is not up).
fn reorder_song_parts(rig: Option<RigClient>, song: String, want: Vec<usize>) {
    let Some(r) = rig else { return };
    let _ = dioxus_core::spawn_forever(async move {
        let mut now: Vec<usize> = (0..want.len()).collect();
        for (pos, &part) in want.iter().enumerate() {
            let Some(at) = now.iter().position(|&p| p == part) else { continue };
            if at != pos {
                let _ = r.edit_song_part(song.clone(), "move".to_string(), at.to_string(), pos.to_string()).await;
                let p = now.remove(at);
                now.insert(pos, p);
            }
        }
    });
}

/// Play song `song` of the set from part `part`.
fn go_to(rig: Option<RigClient>, song: usize, part: usize) {
    let Some(r) = rig else { return };
    let _ = dioxus_core::spawn_forever(async move {
        let _ = r.select_song(song as u32).await;
        let _ = r.select_part(part as u32).await;
    });
}

/// Pick a part: in Build (or with the browser on call), it is what the
/// browser builds into; `open` opens the browser for it.
#[derive(Clone, Copy)]
pub struct PickPart {
    pub open: Callback<()>,
}

#[component]
fn SectionRow(perf: PerformanceModel, lib: LibraryModel, parts: Vec<PerfPart>, song: usize, section: Section, index: usize, count: usize, names: Vec<String>, panel: Signal<Option<Panel>>, on_grip: EventHandler<PointerEvent>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let build_pick = try_use_context::<super::BuildPick>();
    let pick_part = try_use_context::<PickPart>();
    let at = perf.song_index as usize;
    let up = song == at;
    let now = perf.part_index as usize;
    let first = section.parts.first().copied().unwrap_or(0);
    let state = if !up {
        if song < at { "done" } else { "ahead" }
    } else if section.parts.contains(&now) {
        "now"
    } else if first < now {
        "done"
    } else {
        "ahead"
    };
    let is_now = state == "now";
    let several = section.parts.len() > 1;
    // The part being built into is ringed (a one-part section's).
    let picked = up && !several && build_pick.is_some_and(|b| (b.part)() == Some(first));
    let part = parts.get(first).cloned().unwrap_or_default();
    let colour = perf.songs.get(song).map(|s| song_colour(&s.name, &s.colour)).unwrap_or_default();
    let profile = if part.profile.is_empty() { perf.profile_name.clone() } else { part.profile.clone() };
    let stack = stack_for(&perf, &lib, &profile, &part.patch);
    let borrowed = (!part.profile.is_empty()).then(|| part.profile.clone());
    let modules = part_modules(&parts, &section.parts);
    let part_names: Vec<String> = section.parts.iter().filter_map(|&k| parts.get(k).map(|p| p.name.clone())).collect();
    let grouped = !part.section.is_empty();
    let song_name = perf.songs.get(song).map(|s| s.name.clone()).unwrap_or_default();

    let mut items = vec![
        Item::head(section.name.clone()),
        Item::run("go", "Play from here").unless(is_now.then(|| "It's playing".to_string())),
    ];
    // The song up's sound is picked here; any song's sections are edited.
    if up {
        items.push(if several {
            Item::run("parts", "Its parts").detail(format!("{}", section.parts.len()))
        } else {
            Item::run("patch", "Patch…").detail(sound_of(&part).unwrap_or_else(|| "keeps".to_string()))
        });
    }
    {
    items.extend([
        Item::name("rename", "Rename…", section.name.clone(), "Rename", names.clone()),
        Item::Sep,
        Item::run("earlier", "Move earlier").unless((index == 0).then(|| "Already first".to_string())),
        Item::run("later", "Move later").unless((index + 1 == count).then(|| "Already last".to_string())),
        Item::Sep,
        Item::delete("delete", "Delete section"),
    ]);
    }
    let ink = match state {
        "done" => INK_3,
        "now" => INK,
        _ => INK_2,
    };
    let grip = section_colour(&section.name);
    let row_bg = pick(is_now, tint(&colour, pick(several, 10, 18)), CLEAR.to_string());
    // The section playing stays under the song while its stacks scroll.
    let pin = if is_now { format!("{STICK_SECTION} box-shadow: 0 1px 0 {RULE};") } else { UNSTUCK.to_string() };
    let order: Vec<Section> = {
        let mut v = sections_of(&parts);
        v.truncate(count);
        v
    };

    // Pick the part and open the browser on it (the patch picker when
    // there is no browser here).
    let choose = move || {
        if let (Some(b), Some(pp)) = (build_pick, pick_part) {
            let mut part = b.part;
            part.set(Some(first));
            pp.open.call(());
        } else {
            let mut panel = panel;
            panel.set(Some(Panel::Patch(first)));
        }
    };
    let pick_handler = EventHandler::new({
        let rig = rig.clone();
        let part_names = part_names.clone();
        move |p: Picked| {
            let part_names = part_names.clone();
            match p.id.as_str() {
                "go" if !up => go_to(rig.clone(), song, first),
                "go" | "parts" => call!(rig, |r| r.select_part(first as u32)),
                "patch" => choose(),
                "rename" if !up => {
                    let (song, new_name) = (song_name.clone(), p.text.clone());
                    for n in if grouped { part_names } else { part_names.into_iter().take(1).collect() } {
                        let (song, new_name, op) = (song.clone(), new_name.clone(), if grouped { "section" } else { "rename" });
                        call!(rig, |r| r.edit_song_part(song, op.to_string(), n, new_name));
                    }
                }
                "earlier" if !up => reorder_song_parts(rig.clone(), song_name.clone(), section_moved(&order, index, index - 1)),
                "later" if !up => reorder_song_parts(rig.clone(), song_name.clone(), section_moved(&order, index, index + 1)),
                "delete" if !up => {
                    for n in part_names {
                        let song = song_name.clone();
                        call!(rig, |r| r.edit_song_part(song, "remove".to_string(), n, String::new()));
                    }
                }
                "rename" => {
                    let new_name = p.text.clone();
                    if grouped {
                        for n in part_names {
                            let new_name = new_name.clone();
                            call!(rig, |r| r.set_part_section(n, new_name));
                        }
                    } else if let Some(n) = part_names.first().cloned() {
                        call!(rig, |r| r.rename_part(n, new_name));
                    }
                }
                "earlier" => reorder_parts(rig.clone(), section_moved(&order, index, index - 1)),
                "later" => reorder_parts(rig.clone(), section_moved(&order, index, index + 1)),
                "delete" => {
                    for n in part_names {
                        call!(rig, |r| r.remove_part(n));
                    }
                }
                _ => {}
            }
        }
    });
    rsx! {
        PressMenu {
            items: items.clone(),
            on_pick: pick_handler,
            style: "{pin} display: flex; align-items: center; min-height: 50px; background: {row_bg};",
            // The grip, in the section's colour: drag to move the section.
            span {
                "aria-label": "Drag {section.name} to move it",
                style: "width: 40px; align-self: stretch; flex-shrink: 0; display: flex; align-items: center; justify-content: flex-end; padding-right: 6px; box-sizing: border-box; cursor: grab; touch-action: none;",
                onpointerdown: move |e: PointerEvent| {
                    e.prevent_default();
                    e.stop_propagation();
                    on_grip.call(e);
                },
                svg { width: "14", height: "12", view_box: "0 0 14 12",
                    path { d: "M1.5 2h11M1.5 6h11M1.5 10h11", stroke: "{grip}", stroke_opacity: if state == "done" { "0.45" } else { "1" }, stroke_width: "1.7", stroke_linecap: "round" }
                }
            }
            button {
                style: "position: relative; flex: 1; min-width: 0; display: flex; align-items: center; justify-content: flex-start; gap: 12px; min-height: 50px; padding: 4px 4px 4px 6px; text-align: left; background: transparent; border: none; border-radius: {R}; color: inherit; font: inherit; cursor: pointer;",
                onclick: {
                    let rig = rig.clone();
                    move |_| {
                        // Building: a tap picks the section to build into (one
                        // with several parts plays — its parts are picked).
                        if !up {
                            go_to(rig.clone(), song, first);
                            return;
                        }
                        if let Some(b) = build_pick
                            && *b.building.peek()
                            && !several
                        {
                            choose();
                            return;
                        }
                        call!(rig, |r| r.select_part(first as u32))
                    }
                },
                if picked {
                    span { style: "position: absolute; left: 0; top: 0; right: 0; bottom: 0; border: 1.5px solid {FOCUS_FG}; border-radius: {R}; box-sizing: border-box; pointer-events: none;" }
                }
                span { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px;",
                    span { style: "min-width: 0; font-size: 15px; font-weight: {pick(is_now, 700, 520)}; color: {ink}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                        "{section.name}"
                    }
                    Changes { modules: modules.clone(), max: 4 }
                }
                // What it plays.
                span { style: "flex: 0 1 auto; max-width: 64%; min-width: 0; display: flex; padding-right: 10px;",
                    if several {
                        span { style: "font-size: 13px; padding: 3px 8px; border-radius: 4px; background: {FILL}; color: {INK_2}; font-weight: 600;", "{section.parts.len()} parts" }
                    } else if !part.stack.is_empty() {
                        // A whole stack.
                        PatchChip { patch: "the stack".to_string(), stack: part.stack.clone(), small: false, lit: is_now, borrowed: borrowed.clone() }
                    } else if !part.preset.is_empty() {
                        // A preset's variation.
                        PatchChip { patch: part.preset.clone(), stack: String::new(), small: false, lit: is_now, borrowed: borrowed.clone() }
                    } else if !part.patch.is_empty() {
                        PatchChip { patch: part.patch.clone(), stack: stack.clone(), small: false, lit: is_now, borrowed: borrowed.clone() }
                    } else {
                        span { style: "font-size: 13px; color: {INK_3}; padding: 0 4px;", "keeps" }
                    }
                }
            }
            MoreButton {
                items: items.clone(),
                label: format!("{} actions", section.name),
                on_pick: pick_handler,
            }
        }
    }
}

// ── The stacks ─────────────────────────────────────────────────────────────

/// The song's stacks under the section playing (under the song when it has
/// none): a tap plays a stack (a tap on the one playing steps it); each
/// shows the patch a tap plays, its rotation, and what the next tap plays.
#[component]
pub fn Stacks(perf: PerformanceModel, lib: LibraryModel, left: u32) -> Element {
    let views = song_stacks(&perf, &lib);
    let own = part_patch(&perf);
    let live = is_live(&perf, own.as_deref());
    let where_ = where_label(&perf);
    let colour = perf.songs.get(perf.song_index as usize).map_or_else(|| name_colour(&perf.profile_name).to_string(), |s| song_colour(&s.name, &s.colour));
    let part = perf.parts.get(perf.part_index as usize).cloned().unwrap_or_default();
    let in_song = !perf.parts.is_empty();
    let count = views.len();
    rsx! {
        div { style: "position: relative; padding: 2px 0 8px {left}px;",
            div { style: "background: rgba(0,0,0,0.18); display: flex; flex-direction: column; border-bottom: 1px solid {RULE};",
                for (k, v) in views.iter().enumerate() {
                    StackRow {
                        key: "{v.name}",
                        view: v.clone(),
                        shown: k,
                        count,
                        prev: k.checked_sub(1).map(|p| views[p].index),
                        next: views.get(k + 1).map(|n| n.index),
                        lib: lib.clone(),
                        profile: perf.profile_name.clone(),
                        part: part.clone(),
                        own: own.clone(),
                        live,
                        where_: where_.clone(),
                        song_colour: colour.clone(),
                        in_song,
                    }
                }
            }
        }
    }
}

#[component]
pub fn StackRow(
    view: StackView,
    /// Its place among the stacks shown, of `count`; the stacks shown either
    /// side of it, by index — where Move up / down take it.
    shown: usize,
    count: usize,
    prev: Option<usize>,
    next: Option<usize>,
    lib: LibraryModel,
    profile: String,
    /// The part up, and the patch it plays.
    part: PerfPart,
    own: Option<String>,
    /// Something else plays by hand.
    live: bool,
    where_: String,
    song_colour: String,
    in_song: bool,
    /// Drag the stack into order (the Profile view).
    grip: Option<EventHandler<PointerEvent>>,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let index = view.index;
    let on = view.on;
    let n = view.patches.len();
    // The part's own patch, while something else plays by hand: home.
    let saved = own.as_ref().and_then(|o| view.patches.iter().position(|(p, _)| p.eq_ignore_ascii_case(o)));
    let home = live && saved.is_some();
    let by_hand = on && live;
    let pos = if on { view.cursor } else { saved.filter(|_| home).unwrap_or(view.cursor) };
    let showing = view.patches.get(pos).cloned();
    let showing_name = showing.as_ref().map(|s| s.0.clone()).unwrap_or_default();
    let is_default = saved.is_some_and(|s| s == pos);
    let next_patch = (n > 1).then(|| view.patches[(pos + 1) % n].0.clone());
    let tape = tape_for(&view.name);
    // Only the profile's own patches are the profile's to rename or remove.
    let mine = showing.as_ref().is_some_and(|(_, src)| matches!(src, Src::Profile(_)));
    let names: Vec<String> = view.patches.iter().map(|p| p.0.clone()).collect();
    let stack_names: Vec<String> = lib.profiles.iter().find(|p| p.name.eq_ignore_ascii_case(&profile)).map(|p| p.stacks.clone()).unwrap_or_default();

    let head = if showing.is_some() { format!("{} · {showing_name}", view.name) } else { format!("{} · empty", view.name) };
    let mut items = vec![Item::head(head)];
    for (k, (p, src)) in view.patches.iter().enumerate() {
        items.push(Item::run(format!("p{k}"), p.clone()).detail(src.label()).checked(k == pos && on));
    }
    if by_hand {
        items.push(Item::Sep);
        items.push(Item::run("keep", format!("Keep for {where_}")));
        items.push(Item::run("back", format!("Back to {where_}'s patch")));
    }
    // The section (or part) playing gets this patch as its own.
    if in_song && showing.is_some() {
        items.push(Item::Sep);
        items.push(Item::run("default", format!("Make “{showing_name}” default for {where_}")).unless(is_default.then(|| format!("It's {where_}'s already"))));
    }
    items.push(Item::Sep);
    items.push(Item::name("add", "Add a patch…", format!("{} {}", view.name, n + 1), "Add", names.clone()));
    if mine {
        items.push(Item::name("rename_patch", format!("Rename “{showing_name}”…"), showing_name.clone(), "Rename", names.clone()));
    }
    items.push(Item::name("rename", "Rename stack…", view.name.clone(), "Rename", stack_names));
    items.push(Item::run("up", "Move up").unless((shown == 0).then(|| "Already first".to_string())));
    items.push(Item::run("down", "Move down").unless((shown + 1 == count).then(|| "Already last".to_string())));
    if mine {
        items.push(Item::Sep);
        items.push(Item::delete("remove_patch", format!("Remove “{showing_name}”")).unless((n <= 1).then(|| "A stack keeps at least one patch".to_string())));
    }

    // Green bar: what plays (amber when picked by hand). A ring: the part's
    // own patch, while something else plays — tap it to go back.
    let fill = if on { format!("background: {};", pick(by_hand, MODIFIED, LIVE)) } else { String::new() };
    let ring = if home { format!("border: 1.25px solid {LIVE}; box-sizing: border-box;") } else { String::new() };
    let pick_handler = EventHandler::new({
        let rig = rig.clone();
        let (part, own, showing, rig_at) = (part.clone(), own.clone(), showing.clone(), view.at.clone());
        let stack_name = view.name.clone();
        move |p: Picked| {
            let stack_name = stack_name.clone();
            match p.id.as_str() {
                "keep" | "default" => {
                    if let Some((name, src)) = showing.clone() {
                        let borrowed = match src {
                            Src::Other(p) => Some(p),
                            _ => None,
                        };
                        give_part(rig.clone(), &part, name, borrowed);
                    }
                }
                "back" => {
                    if let Some(o) = own.clone() {
                        play_patch(rig.clone(), o);
                    }
                }
                "add" => {
                    let name = p.text.clone();
                    call!(rig, |r| r.set_stack_patch(stack_name, name, true));
                }
                "rename_patch" => {
                    if let Some((old, _)) = showing.clone() {
                        let new_name = p.text.clone();
                        call!(rig, |r| r.rename_patch(old, new_name));
                    }
                }
                "rename" => {
                    let new_name = p.text.clone();
                    call!(rig, |r| r.rename_stack(stack_name, new_name));
                }
                "up" => {
                    if let Some(to) = prev {
                        call!(rig, |r| r.move_stack(index as u32, to as u32));
                    }
                }
                "down" => {
                    if let Some(to) = next {
                        call!(rig, |r| r.move_stack(index as u32, to as u32));
                    }
                }
                "remove_patch" => {
                    if let Some((name, _)) = showing.clone() {
                        call!(rig, |r| r.set_stack_patch(stack_name, name, false));
                    }
                }
                other => {
                    // Land the stack on that patch and play it.
                    if let Some(k) = other.strip_prefix('p').and_then(|k| k.parse::<usize>().ok())
                        && let Some(&at) = rig_at.get(k)
                    {
                        call!(rig, |r| r.play_stack_patch(index as u32, at as u32));
                    }
                }
            }
        }
    });
    let label = if showing.is_some() { showing_name.clone() } else { "Empty".to_string() };
    let grip_ink = super::colors::tape_mark(&view.name);
    rsx! {
        PressMenu {
            items: items.clone(),
            on_pick: pick_handler,
            style: "display: flex; align-items: stretch; border-top: 1px solid {RULE}; background: {pick(on, tint(&song_colour, 18), CLEAR.to_string())};",
            if let Some(g) = grip {
                span {
                    "aria-label": "Drag {view.name} to move it",
                    style: "width: 36px; flex-shrink: 0; display: flex; align-items: center; justify-content: center; cursor: grab; touch-action: none;",
                    onpointerdown: move |e: PointerEvent| {
                        e.prevent_default();
                        e.stop_propagation();
                        g.call(e);
                    },
                    svg { width: "14", height: "12", view_box: "0 0 14 12",
                        path { d: "M1.5 2h11M1.5 6h11M1.5 10h11", stroke: grip_ink, stroke_width: "1.7", stroke_linecap: "round" }
                    }
                }
            }
            button {
                disabled: n == 0,
                style: "position: relative; flex: 1; min-width: 0; display: flex; align-items: center; justify-content: flex-start; gap: 10px; min-height: 44px; padding: {pick(grip.is_some(), \"0 4px 0 2px\", \"0 4px 0 10px\")}; text-align: left; background: transparent; border: none; color: inherit; font: inherit; cursor: pointer;",
                onclick: {
                    let rig = rig.clone();
                    let own = own.clone();
                    move |_| {
                        // Home: back to the part's own patch. Else: play the stack.
                        if home {
                            if let Some(o) = own.clone() {
                                play_patch(rig.clone(), o);
                            }
                        } else {
                            call!(rig, |r| r.press_stack(index as u32));
                        }
                    }
                },
                if on || home {
                    span { style: "position: absolute; left: 0; top: 6px; bottom: 6px; width: 3px; border-radius: 2px; {fill} {ring}" }
                }
                span { style: "width: 70px; flex-shrink: 0; display: flex; align-items: center; gap: 6px;",
                    span { style: "width: 8px; height: 8px; border-radius: 2px; flex-shrink: 0; background: {tape};" }
                    span { style: "font-size: 11px; font-weight: 700; letter-spacing: 0.08em; text-transform: uppercase; white-space: nowrap; overflow: hidden; color: {pick(on, INK, INK_3)};", "{view.name}" }
                }
                span { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px;",
                    span { style: "font-size: 14px; font-weight: {pick(on, 700, 560)}; color: {pick(on, INK, INK_2)}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                        "{label}"
                    }
                    if n > 1 {
                        span { style: "display: flex; align-items: center; gap: 4px; min-width: 0;",
                            for (k, (p, src)) in view.patches.iter().enumerate() {
                                RotationDot { key: "{p}", lit: k == pos, source: src.clone(), song_colour: song_colour.clone() }
                            }
                            // What the next tap plays, on the stack that's playing.
                            if on && let Some(nx) = next_patch.clone() {
                                span { style: "margin-left: 6px; min-width: 0; font-size: 12px; font-weight: 600; color: {INK_3}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                                    "next › "
                                    span { style: "color: {INK_2};", "{nx}" }
                                }
                            }
                        }
                    }
                }
                if is_default {
                    span { style: "flex-shrink: 0; font-size: 12px; font-weight: 650; color: {INK_3};", "Default" }
                }
                // Borrowed from another profile: that profile, by name.
                if let Some((_, Src::Other(b))) = showing.clone() {
                    span { style: "flex-shrink: 0; font-size: 12px; font-weight: 650; color: {name_colour(&b)};", "{b}" }
                }
            }
            MoreButton {
                items: items.clone(),
                label: format!("{} actions", view.name),
                on_pick: pick_handler,
            }
        }
    }
}

/// A patch in a stack's rotation: the song's own filled in the song's
/// colour, one borrowed in its profile's, the profile's own open.
#[component]
fn RotationDot(lit: bool, source: Src, song_colour: String) -> Element {
    let style = match &source {
        Src::Song => format!("background: {song_colour}; opacity: {};", pick(lit, 1.0, 0.55)),
        Src::Other(p) => format!("background: {};", name_colour(p)),
        Src::Profile(_) if lit => format!("background: {INK_2};"),
        Src::Profile(_) => format!("border: 1.25px solid {INK_3}; box-sizing: border-box;"),
    };
    rsx! {
        span { style: "width: {pick(lit, 12, 5)}px; height: 5px; border-radius: 999px; flex-shrink: 0; {style}" }
    }
}
