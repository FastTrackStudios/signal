//! The instrument menu — the app's front door.
//!
//! The catalogue itself (what rigs exist, their slugs, labels and blurbs)
//! lives in [`signal_rigs_proto::Rig`], which the engine also reads for its
//! router scopes — one authority, so a rig cannot be called one thing on the
//! wire and another on screen. What stays here is what is genuinely this
//! binary's business:
//!
//! - [`available`] — which rigs *this build* can open. The phone runs its
//!   rigs in-process and only two of them compile for iOS; the desktop
//!   reaches every one through the engine.
//! - [`icon`] — the rail glyph, an app UI concern.
//! - [`RigMenu`] — the menu itself, rendered by both the desktop workspace
//!   (`rig_view`) and the phone shell (`mobile_view`).
//!
//! Guide is deliberately *not* a rig. It belongs to the session domain
//! (click, count-in, section cues); until it is here it waits in the front
//! page's "Coming" line with the rigs still to come.

use dioxus::prelude::*;
pub use signal_rigs_proto::Rig;

use crate::prefs;

/// Whether this rig actually runs in this build — see the module note.
#[cfg(target_os = "ios")]
pub fn available(rig: Rig) -> bool {
    match rig {
        // The bass is the guitar rig as another instrument.
        Rig::Guitar | Rig::Bass => cfg!(feature = "signal-guitar"),
        // The drums' pads and kit (the engine joins them on iOS later).
        Rig::Drums => cfg!(feature = "signal-guitar"),
        // Not on the phone for now: the keys rig does not work there yet,
        // so its tile is greyed out rather than opening onto it.
        Rig::Keys => false,
        _ => false,
    }
}

/// See the iOS arm above.
#[cfg(not(target_os = "ios"))]
pub const fn available(rig: Rig) -> bool {
    match rig {
        // No vocal chain engine exists yet.
        Rig::Vocals => false,
        _ => cfg!(feature = "signal"),
    }
}

/// The rail glyph. `fts_chrome` has no vocal glyph yet, so Vocals borrows the
/// perform icon until one lands in that repo.
pub const fn icon(rig: Rig) -> fts_chrome::Icon {
    use fts_chrome::Icon;
    match rig {
        Rig::Guitar => Icon::Guitar,
        Rig::Keys => Icon::Keys,
        Rig::Drums | Rig::Ekit => Icon::Drums,
        Rig::Bass => Icon::Bass,
        Rig::Vocals => Icon::Perform,
        Rig::Synth => Icon::Synth,
        Rig::Space => Icon::Browser,
    }
}

/// Guide is a session surface, not a rig: click, count-in and spoken section
/// cues.
///
/// It keeps its place in the menu because that is where a player looks for it
/// — you start the click before you pick up an instrument — but this binary is
/// Signal only, and the session domain lives in another repo. So the card is
/// shown and marked plainly as not here yet, which beats omitting it and
/// leaving someone hunting for a thing they remember.
pub const GUIDE_AVAILABLE: bool = false;

// ── Remembering where you were ──────────────────────────────────────────────

/// The rig to open on launch: an explicit request first, then the remembered
/// one. `None` means show the menu — which is what a first-ever launch gets.
pub fn load_last() -> Option<Rig> {
    // An explicit request wins over the remembered rig: `--rig keys` (or
    // `FTS_OPEN_RIG=keys`) is someone saying "open here now", and having it
    // lose to last session's choice would make the flag useless exactly when
    // you reach for it.
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(k) = std::env::var("FTS_OPEN_RIG")
        .ok()
        .and_then(|s| Rig::from_slug(s.trim()))
    {
        return Some(k);
    }
    #[cfg(target_arch = "wasm32")]
    {
        let hash = web_sys::window()
            .and_then(|w| w.location().hash().ok())
            .unwrap_or_default();
        if let Some(rig) = hash.trim_start_matches('#').split('/').nth(1) {
            if let Some(k) = Rig::from_slug(rig) {
                return Some(k);
            }
        }
    }
    // A remembered rig that this build cannot open would strand the app on a
    // dead screen, so it falls back to the menu.
    prefs::get("last-rig")
        .as_deref()
        .and_then(Rig::from_slug)
        .filter(|k| available(*k))
}

pub fn store_last(rig: Option<Rig>) {
    match rig {
        Some(k) => {
            prefs::set("last-rig", k.slug());
            // Kept when you go back to the menu (unlike `last-rig`): the
            // front page lights it and offers Resume.
            prefs::set("last-played", k.slug());
        }
        None => prefs::remove("last-rig"),
    }
}

/// The rig played last — the one the front page lights, even after going
/// back to the menu.
pub fn last_played() -> Option<Rig> {
    prefs::get("last-played").as_deref().and_then(Rig::from_slug).filter(|k| available(*k))
}

// ── The menu ────────────────────────────────────────────────────────────────

/// What a ready rig will open on: its profile and what plays there.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RigTile {
    /// The line under the name ("Worship · Glassy", "Organic Oomph").
    pub opens_on: String,
    /// Its engine is up and making sound.
    pub live: bool,
    /// What is on the board: each stack and the patch it lands on, and
    /// whether it is the one up.
    pub stacks: Vec<(String, String, bool)>,
}

/// One song of tonight's set.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SetSong {
    pub name: String,
    pub key: String,
    pub bpm: u32,
}

/// Tonight's set, as the guitar rig holds it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SetInfo {
    pub name: String,
    /// When it is played ("6 Oct"), empty when it has no day.
    pub day: String,
    pub songs: Vec<SetSong>,
    /// The song that is up.
    pub up: usize,
}

/// Everything the front page shows besides the catalogue — gathered by the
/// shell (`front_state`), so the menu stays plain data and draws the same in
/// every build, the browser's included.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrontState {
    pub tiles: Vec<(Rig, RigTile)>,
    pub set: Option<SetInfo>,
    /// Where the sound comes from: "This iPad", or the studio's address.
    pub studio: String,
    /// The studio answered.
    pub connected: bool,
}

impl FrontState {
    fn tile(&self, rig: Rig) -> RigTile {
        self.tiles.iter().find(|(r, _)| *r == rig).map(|(_, t)| t.clone()).unwrap_or_default()
    }
}

// The page's own values (Signal Touch, DESIGN.md).
const DESK: &str = "#0a0a0c";
const SHEET: &str = "#0e0e11";
const RULE: &str = "#222228";
const RULE_STRONG: &str = "#2b2b31";
const INK: &str = "#e4e4e7";
const INK_2: &str = "#a1a1aa";
const INK_3: &str = "#80808a";
const DIM: &str = "#3f3f46";
const LIVE: &str = "#22c55e";
const FONT: &str = "-apple-system, 'SF Pro Text', system-ui, 'Segoe UI', Roboto, 'Helvetica Neue', sans-serif";

const PRIMARY: &str = "#2563eb";
const LIVE_BG: &str = "rgba(34, 197, 94, 0.12)";
/// A picked thing that is not sounding: the dark blue wash.
const UP: &str = "#1b2331";
/// The phone's camera housing, kept clear on its side (the rig's own value).
const HOUSING: u32 = 54;

/// The front page. Left: where you were, one press from playing again, and
/// the other ready rigs under it, each saying what it opens on. Right:
/// tonight's set as a run sheet — a song is one press from the guitar on it.
/// A slim bar on top says where the sound comes from; the Libraries are a
/// press away; the rigs still to come wait in one quiet line.
#[component]
pub fn RigMenu(
    on_pick: EventHandler<Rig>,
    phone: bool,
    #[props(default)] on_libraries: Option<EventHandler<()>>,
    #[props(default)] on_song: Option<EventHandler<usize>>,
    #[props(default)] on_setlists: Option<EventHandler<()>>,
    #[props(default)] front: FrontState,
    /// Taller than wide (an iPad held upright): the set under the rigs.
    #[props(default)] portrait: bool,
) -> Element {
    // Bass sits by the guitar it is a mode of; the rest follow the catalogue.
    let mut ready: Vec<Rig> = Rig::ALL.iter().copied().filter(|r| available(*r)).collect();
    ready.sort_by_key(|r| match r {
        Rig::Guitar => 0,
        Rig::Bass => 1,
        Rig::Drums => 2,
        _ => 3,
    });
    let coming: Vec<&'static str> = Rig::ALL
        .iter()
        .copied()
        .filter(|r| !available(*r))
        .map(Rig::label)
        .chain((!GUIDE_AVAILABLE).then_some("Guide"))
        .collect();
    let last = last_played().filter(|r| ready.contains(r)).or_else(|| ready.first().copied());
    let others: Vec<Rig> = ready.iter().copied().filter(|r| Some(*r) != last).collect();
    // A phone in landscape has about 330pt under the bar: tighter, and the
    // Resume pane drops its stacks (the song line says enough).
    let pad = if phone { 14 } else { 22 };
    let gap = if phone { 10 } else { 14 };
    // On a phone on its side: the camera housing's side kept clear, and the
    // foot clear of the rounded corners and the home indicator.
    let island_left = try_use_context::<signal_guitar_ui::IslandLeft>().map(|s| (s.0)());
    let corner = try_use_context::<signal_guitar_ui::ScreenCorners>().map_or(0.0, |c| (c.0)());
    let (lead, trail) = match (phone, island_left) {
        (true, Some(true)) => (HOUSING, pad),
        (true, Some(false)) => (pad, HOUSING),
        _ => (pad, pad),
    };
    let foot = if phone && corner > 0.0 { (corner * 0.35).round() as u32 } else { gap };
    let guitar_live = front.tile(Rig::Guitar).live;
    // Side by side, the rigs no wider than a hand's reach; upright, stacked,
    // each as tall as it needs and the set taking the rest.
    let (direction, left) = if portrait {
        ("column", "flex: 0 0 auto; max-height: 60%;")
    } else {
        ("row", "flex: 3 1 0; max-width: 760px; height: 100%;")
    };
    rsx! {
        div { style: "flex: 1; min-height: 0; height: 100%; display: flex; flex-direction: column; background: {DESK}; color: {INK}; font-family: {FONT}; font-variant-numeric: tabular-nums;",
            document::Style { {PRESS_CSS} }
            Bar { front: front.clone(), lead, trail, on_libraries }
            div { style: "flex: 1; min-height: 0; display: flex; flex-direction: {direction}; gap: {gap}px; padding: {gap}px {trail}px {foot}px {lead}px; box-sizing: border-box;",
                // Where you were, and the other rigs — no wider than a hand's
                // reach on a big screen; the set takes the rest.
                div { style: "{left} min-width: 0; display: flex; flex-direction: column; gap: {gap}px; overflow-y: auto;",
                    if let Some(rig) = last {
                        Resume { rig, tile: front.tile(rig), set: front.set.clone(), phone, on_pick: move |()| on_pick.call(rig) }
                    }
                    if !others.is_empty() {
                        div { style: "flex-shrink: 0; display: flex; flex-direction: column; border: 1px solid {RULE}; border-radius: 8px; background: {SHEET}; overflow: hidden;",
                            for (i, rig) in others.into_iter().enumerate() {
                                RigRow { key: "{rig.slug()}", rig, tile: front.tile(rig), first: i == 0, phone, on_pick: move |()| on_pick.call(rig) }
                            }
                        }
                    }
                    if !portrait {
                        span { style: "flex: 1;" }
                    }
                    if !coming.is_empty() {
                        div { style: "flex-shrink: 0; display: flex; align-items: baseline; gap: 10px; padding: 0 4px; white-space: nowrap; overflow: hidden;",
                            span { style: "font-size: 12px; font-weight: 700; letter-spacing: 0.1em; text-transform: uppercase; color: {INK_3};", "Coming" }
                            span { style: "font-size: 14px; color: {INK_3}; overflow: hidden; text-overflow: ellipsis;", "{coming.join(\" · \")}" }
                        }
                    }
                }
                // Tonight.
                RunSheet { set: front.set.clone(), phone, live: guitar_live, on_song, on_setlists }
            }
        }
    }
}

/// The bar: the wordmark, where the sound comes from, the Libraries.
#[component]
fn Bar(front: FrontState, lead: u32, trail: u32, on_libraries: Option<EventHandler<()>>) -> Element {
    let dot = if front.connected { LIVE } else { DIM };
    rsx! {
        div { style: "flex-shrink: 0; height: 56px; display: flex; align-items: center; gap: 16px; padding: 0 {trail}px 0 {lead}px; border-bottom: 1px solid {RULE_STRONG}; background: {SHEET}; box-sizing: border-box;",
            span { style: "font-size: 24px; font-weight: 750; letter-spacing: -0.02em;", "signal" }
            span { style: "width: 1px; height: 24px; background: {RULE};" }
            if !front.studio.is_empty() {
                span { style: "display: flex; align-items: center; gap: 8px; font-size: 15px; color: {INK_2}; white-space: nowrap;",
                    span { style: "width: 8px; height: 8px; border-radius: 4px; background: {dot};" }
                    "{front.studio}"
                }
            }
            span { style: "flex: 1;" }
            if let Some(open) = on_libraries {
                button {
                    class: "fp-press",
                    style: "height: 44px; display: flex; align-items: center; gap: 10px; padding: 0 16px; border: 1px solid {RULE_STRONG}; border-radius: 6px; background: transparent; color: {INK}; font-family: {FONT}; font-size: 15px; font-weight: 650; cursor: pointer;",
                    onclick: move |_| open.call(()),
                    svg { width: "18", height: "18", view_box: "0 0 18 18",
                        path { d: "M3 3.5h3v11H3ZM7.5 3.5h3v11h-3ZM12 4l2.8-.8 2 10.6-2.8.8Z", fill: "none", stroke: INK_2, stroke_width: "1.4", stroke_linejoin: "round" }
                    }
                    "Libraries"
                }
            }
        }
    }
}

/// Where you were: the rig, what it plays, the song up — and Resume.
#[component]
fn Resume(rig: Rig, tile: RigTile, set: Option<SetInfo>, phone: bool, on_pick: EventHandler<()>) -> Element {
    let (pad, gap, name, line, button) = if phone { ("12px 18px", 6, 30, 16, 48) } else { ("26px 26px 24px", 18, 44, 22, 56) };
    let opens_on = if tile.opens_on.is_empty() { rig.blurb().to_string() } else { tile.opens_on.clone() };
    let song = set.as_ref().filter(|_| matches!(rig, Rig::Guitar | Rig::Bass)).and_then(|s| s.songs.get(s.up).cloned());
    let state = if tile.live { "Playing" } else { "Last played" };
    rsx! {
        div { style: "flex-shrink: 0; display: flex; flex-direction: column; gap: {gap}px; padding: {pad}; border: 1px solid {RULE_STRONG}; border-radius: 8px; background: {SHEET}; box-sizing: border-box;",
            div { style: "display: flex; align-items: center; gap: 16px; min-width: 0;",
            div { style: "flex: 1; display: flex; flex-direction: column; gap: 6px; min-width: 0;",
                // The rig, and on its line whether it is playing.
                span { style: "display: flex; align-items: baseline; gap: 14px; min-width: 0;",
                    span { style: "font-size: {name}px; font-weight: 750; letter-spacing: -0.02em; line-height: 1.08;", "{rig.label()}" }
                    span { style: "display: flex; align-items: center; gap: 8px; white-space: nowrap;",
                        span { style: "width: 10px; height: 10px; border-radius: 5px; background: {pick_dot(tile.live)};" }
                        span { style: "font-size: 12px; font-weight: 700; letter-spacing: 0.1em; text-transform: uppercase; color: {pick_state(tile.live)};", "{state}" }
                    }
                }
                span { style: "font-size: {line}px; font-weight: 650; color: {INK_2}; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;", "{opens_on}" }
                if let Some(s) = song {
                    span { style: "font-size: 15px; color: {INK_3}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                        "Song up: "
                        span { style: "color: {INK}; font-weight: 700;", "{s.name}" }
                        // The phone's run sheet beside it gives the key and tempo.
                        if !phone && !s.key.is_empty() { " · {s.key}" }
                        if !phone && s.bpm > 0 { " · {s.bpm} BPM" }
                    }
                }
            }
            // On a phone, Resume sits beside the name: the height goes to the rigs.
            if phone {
                ResumeButton { rig, height: button, on_pick }
            }
            }
            if !phone && !tile.stacks.is_empty() {
                div { style: "display: flex; flex-wrap: wrap; gap: 8px;",
                    for (i, (stack, patch, up)) in tile.stacks.iter().take(6).enumerate() {
                        {
                            let (swatch, _) = signal_guitar_ui::folder_color(stack);
                            let up = *up;
                            rsx! {
                                // DESIGN's stack chip: a 20% tint and a swatch; the
                                // one up set bolder, nothing boxed.
                                span { key: "{i}", style: "height: 30px; display: flex; align-items: center; gap: 8px; padding: 0 10px; border-radius: 4px; background: color-mix(in oklab, {swatch} 20%, transparent); box-sizing: border-box; max-width: 240px;",
                                    span { style: "width: 8px; height: 8px; border-radius: 2px; flex-shrink: 0; background: {swatch};" }
                                    span { style: "font-size: 12px; font-weight: 700; letter-spacing: 0.06em; text-transform: uppercase; color: {INK_2}; white-space: nowrap;", "{stack}" }
                                    span { style: "font-size: 15px; font-weight: {weight(up)}; color: {INK}; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;", "{patch}" }
                                }
                            }
                        }
                    }
                }
            }
            if !phone {
                ResumeButton { rig, height: button, on_pick }
            }
        }
    }
}

/// The page's one solid button: back into the rig you left.
#[component]
fn ResumeButton(rig: Rig, height: u32, on_pick: EventHandler<()>) -> Element {
    rsx! {
            button {
                class: "fp-press fp-focus",
                "aria-label": "Resume {rig.label()}",
                style: "align-self: flex-start; height: {height}px; min-width: 200px; flex-shrink: 0; display: flex; align-items: center; justify-content: center; gap: 12px; padding: 0 28px; border: none; border-radius: 6px; background: {PRIMARY}; color: #ffffff; font-family: {FONT}; font-size: 19px; font-weight: 750; cursor: pointer;",
                onclick: move |_| on_pick.call(()),
                svg { width: "16", height: "18", view_box: "0 0 16 18",
                    path { d: "M2 1.5v15l13-7.5Z", fill: "#ffffff" }
                }
                "Resume {rig.label()}"
            }
    }
}

/// Another ready rig: its name, what it opens on, and the way in.
#[component]
fn RigRow(rig: Rig, tile: RigTile, first: bool, phone: bool, on_pick: EventHandler<()>) -> Element {
    let height = if phone { 58 } else { 76 };
    let opens_on = if tile.opens_on.is_empty() { rig.blurb().to_string() } else { tile.opens_on.clone() };
    let edge = if first { "none".to_string() } else { format!("1px solid {RULE}") };
    rsx! {
        button {
            class: "fp-press fp-focus",
            "aria-label": "Open {rig.label()}",
            style: "width: 100%; height: {height}px; flex-shrink: 0; display: flex; align-items: center; gap: 16px; padding: 0 20px 0 22px; border: none; border-top: {edge}; background: transparent; color: {INK}; font-family: {FONT}; text-align: left; cursor: pointer;",
            onclick: move |_| on_pick.call(()),
            span { style: "width: 10px; height: 10px; border-radius: 5px; flex-shrink: 0; background: {pick_dot(tile.live)};" }
            span { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px;",
                span { style: "font-size: 22px; font-weight: 800; letter-spacing: -0.01em;", "{rig.label()}" }
                span { style: "font-size: 14px; color: {INK_3}; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;", "{opens_on}" }
            }
            svg { width: "10", height: "16", view_box: "0 0 10 16",
                path { d: "M2 2l6 6-6 6", fill: "none", stroke: INK_3, stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round" }
            }
        }
    }
}

/// Tonight: the set up, every song with its key and tempo, the one up
/// marked; a song is one press from the guitar on it.
#[component]
fn RunSheet(set: Option<SetInfo>, phone: bool, live: bool, on_song: Option<EventHandler<usize>>, on_setlists: Option<EventHandler<()>>) -> Element {
    let (head, title, row) = if phone { ("12px 16px 10px", 20, 48) } else { ("22px 22px 16px", 28, 60) };
    rsx! {
        div { style: "flex: 2 1 0; min-width: 0; min-height: 0; display: flex; flex-direction: column; border: 1px solid {RULE}; border-radius: 8px; background: {SHEET}; overflow: hidden; box-sizing: border-box;",
            match set {
                Some(set) => rsx! {
                    div { style: "flex-shrink: 0; display: flex; flex-direction: column; gap: 4px; padding: {head}; border-bottom: 1px solid {RULE_STRONG};",
                        span { style: "font-size: {title}px; font-weight: 750; letter-spacing: -0.02em; line-height: 1.1;", "{set.name}" }
                        span { style: "font-size: 14px; color: {INK_3};",
                            if set.day.is_empty() { "Tonight · {set.songs.len()} songs" } else { "{set.day} · {set.songs.len()} songs" }
                        }
                    }
                    div { style: "flex: 1; min-height: 0; overflow-y: auto;",
                        for (i, song) in set.songs.iter().cloned().enumerate() {
                            {
                                let up = i == set.up;
                                rsx! {
                                    button {
                                        key: "{i}",
                                        class: "fp-press fp-focus",
                                        "aria-label": "Play {song.name}",
                                        style: "width: 100%; height: {row}px; flex-shrink: 0; display: flex; align-items: center; gap: 14px; padding: 0 {pad_x(phone)}px; border: none; border-bottom: 1px solid {RULE}; background: {pick_row(up, live)}; color: {INK}; font-family: {FONT}; text-align: left; cursor: pointer;",
                                        onclick: move |_| {
                                            if let Some(h) = on_song {
                                                h.call(i);
                                            }
                                        },
                                        span { style: "width: 22px; flex-shrink: 0; font-size: 14px; font-weight: 700; color: {pick_num(up, live)};", "{i + 1}" }
                                        span { style: "flex: 1; min-width: 0; font-size: 17px; font-weight: {weight(up)}; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;", "{song.name}" }
                                        if !song.key.is_empty() {
                                            span { style: "min-width: 30px; height: 26px; display: flex; align-items: center; justify-content: center; padding: 0 6px; border: 1px solid {RULE_STRONG}; border-radius: 4px; font-size: 13px; font-weight: 700; color: {INK_2}; box-sizing: border-box;", "{song.key}" }
                                        }
                                        span { style: "width: 34px; text-align: right; font-size: 14px; color: {INK_3};",
                                            if song.bpm > 0 { "{song.bpm}" } else { "" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
                None => rsx! {
                    div { style: "display: flex; flex-direction: column; align-items: flex-start; gap: 14px; padding: 22px;",
                        span { style: "font-size: 22px; font-weight: 800; white-space: nowrap;", "No set tonight" }
                        if let Some(open) = on_setlists {
                            button {
                                class: "fp-press fp-focus",
                                style: "height: 48px; padding: 0 18px; border: 1px solid {RULE_STRONG}; border-radius: 6px; background: transparent; color: {INK}; font-family: {FONT}; font-size: 15px; font-weight: 650; cursor: pointer;",
                                onclick: move |_| open.call(()),
                                "Setlists"
                            }
                        }
                    }
                },
            }
        }
    }
}

const fn pad_x(phone: bool) -> u32 {
    if phone { 16 } else { 22 }
}

const fn pick_dot(live: bool) -> &'static str {
    if live { LIVE } else { DIM }
}
const fn pick_state(live: bool) -> &'static str {
    if live { LIVE } else { INK_3 }
}
const fn weight(up: bool) -> u32 {
    if up { 800 } else { 560 }
}
/// The song up: green while the guitar sounds it, else just picked (blue).
const fn pick_row(up: bool, live: bool) -> &'static str {
    match (up, live) {
        (true, true) => LIVE_BG,
        (true, false) => UP,
        _ => "transparent",
    }
}
const fn pick_num(up: bool, live: bool) -> &'static str {
    match (up, live) {
        (true, true) => LIVE,
        (true, false) => INK,
        _ => INK_3,
    }
}

/// A press sinks a little and lightens (inline backgrounds, hence
/// `!important`); keyboard focus takes the pale-blue ring.
const PRESS_CSS: &str = ".fp-press:not(:disabled):active { transform: scale(0.985); background-image: linear-gradient(rgba(255,255,255,0.07), rgba(255,255,255,0.07)) !important; } .fp-focus:focus-visible { outline: 2px solid #bfdbfe; outline-offset: -2px; }";
