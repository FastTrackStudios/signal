//! The drum rig on the iPad: a 4×4 pad grid to play the kit by touch, and
//! the kit's pieces beside it to pick one and see what it is.
//!
//! No footswitches and no routing: the drum rig is played from pads (or
//! MIDI, or triggers on a real kit) and mixed in the session's mixer. Here:
//!
//! - the top bar: the kit, and (two steps in, under ⋯) the way back to the
//!   app's instruments;
//! - the sidebar: the kit's pieces by family — Kick, Snare, Hi-Hat, Toms,
//!   Cymbals, Perc — and the picked piece's inspector at its foot (its
//!   instrument, stepped through the ones like it; mute; solo);
//! - the pads: the kit laid out as an MPC is, from the bottom left — kick,
//!   snare, side stick, clap; the hats and the bell; the toms and the ride;
//!   the cymbals on top. A pad plays as the finger lands (higher on the pad,
//!   harder: an iPad's glass has no pressure) and lets go as it lifts; the
//!   pad hit is the piece picked.
//!
//! The drum rig's own client when the app provides one; otherwise a demo
//! kit, so the surface is there before the engine is.

use dioxus::prelude::*;
use signal_drums_proto::drum::{DrumEvent, DrumRigClient, DrumRigStreamClient};
use signal_drums_proto::{KitInfo, KitSlot, LibraryPiece, MixerStrip, PieceInfo, StripKind};

use super::menu::{Item, MoreButton, Picked};
use super::tokens::*;

/// A piece's family: its colour, its place on the pads, its group in the
/// list.
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
enum Family {
    Kick,
    Snare,
    HiHat,
    Toms,
    Cymbals,
    Perc,
}

impl Family {
    const ALL: [Self; 6] = [Self::Kick, Self::Snare, Self::HiHat, Self::Toms, Self::Cymbals, Self::Perc];

    fn of(id: &str, label: &str) -> Self {
        let s = format!("{id} {label}").to_lowercase();
        let has = |w: &[&str]| w.iter().any(|w| s.contains(w));
        if has(&["kick", "bass drum", "bd"]) {
            Self::Kick
        } else if has(&["hat", "hh"]) {
            Self::HiHat
        } else if has(&["snare", "stick", "rim", "sd"]) {
            Self::Snare
        } else if has(&["tom"]) {
            Self::Toms
        } else if has(&["crash", "ride", "china", "splash", "cym", "bell"]) {
            Self::Cymbals
        } else {
            Self::Perc
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Kick => "Kick",
            Self::Snare => "Snare",
            Self::HiHat => "Hi-Hat",
            Self::Toms => "Toms",
            Self::Cymbals => "Cymbals",
            Self::Perc => "Perc",
        }
    }

    /// The family's colour: an identity mark (a band, a swatch), never a
    /// fill.
    fn colour(self) -> &'static str {
        match self {
            Self::Kick => "#f97316",
            Self::Snare => "#38bdf8",
            Self::HiHat => "#facc15",
            Self::Toms => "#a78bfa",
            Self::Cymbals => "#2dd4bf",
            Self::Perc => "#f472b6",
        }
    }
}

/// One piece as the surface shows it.
#[derive(Clone, PartialEq, Debug)]
struct Piece {
    /// The engine id (`kick`, `rtom1`) — what a swap addresses.
    id: String,
    label: String,
    note: u32,
    /// The instrument in the slot.
    instrument: String,
    kind: String,
    family: Family,
}

/// The pads, bottom-left first (pad 1), as an MPC numbers them: where each
/// family's pieces go, in order.
const PAD_ORDER: [Family; 16] = [
    Family::Kick,
    Family::Snare,
    Family::Snare,
    Family::Perc,
    Family::HiHat,
    Family::HiHat,
    Family::HiHat,
    Family::Cymbals,
    Family::Toms,
    Family::Toms,
    Family::Toms,
    Family::Cymbals,
    Family::Cymbals,
    Family::Cymbals,
    Family::Cymbals,
    Family::Cymbals,
];

/// The kit on the 16 pads: each pad takes the next piece of its family; what
/// is left over fills the pads still empty, in kit order.
fn lay_out(pieces: &[Piece]) -> [Option<usize>; 16] {
    let mut pads = [None; 16];
    let mut used = vec![false; pieces.len()];
    for (pad, fam) in PAD_ORDER.iter().enumerate() {
        if let Some(i) = (0..pieces.len()).find(|&i| !used[i] && pieces[i].family == *fam) {
            used[i] = true;
            pads[pad] = Some(i);
        }
    }
    for pad in pads.iter_mut().filter(|p| p.is_none()) {
        if let Some(i) = (0..pieces.len()).find(|&i| !used[i]) {
            used[i] = true;
            *pad = Some(i);
        }
    }
    pads
}

/// A friendly name from an engine id (`rtom1` → `Rack Tom 1`).
fn label_of(id: &str) -> String {
    let words: Vec<String> = id
        .split(['-', '_', ' '])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next().map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
        })
        .collect();
    words.join(" ")
}

/// The demo kit: sixteen GM pieces, shown when no drum rig is connected.
fn demo_kit() -> Vec<Piece> {
    let p = |id: &str, label: &str, note: u32, instrument: &str, kind: &str| Piece {
        id: id.into(),
        label: label.into(),
        note,
        instrument: instrument.into(),
        kind: kind.into(),
        family: Family::of(id, label),
    };
    vec![
        p("kick", "Kick", 36, "Maple 22×18", "kick"),
        p("snare", "Snare", 38, "Brass 14×6.5", "snare"),
        p("sidestick", "Side Stick", 37, "Brass 14×6.5", "snare"),
        p("clap", "Clap", 39, "Studio Clap", "perc"),
        p("hihat-closed", "Hat Closed", 42, "K 14\"", "hihat"),
        p("hihat-pedal", "Hat Pedal", 44, "K 14\"", "hihat"),
        p("hihat-open", "Hat Open", 46, "K 14\"", "hihat"),
        p("ride-bell", "Ride Bell", 53, "K Light 22\"", "ride"),
        p("rtom1", "Rack Tom 1", 48, "Maple 10\"", "tom"),
        p("rtom2", "Rack Tom 2", 45, "Maple 12\"", "tom"),
        p("ftom1", "Floor Tom", 43, "Maple 16\"", "tom"),
        p("ride", "Ride", 51, "K Light 22\"", "ride"),
        p("crash-l", "Crash L", 49, "A Custom 18\"", "crash"),
        p("crash-r", "Crash R", 57, "A Custom 19\"", "crash"),
        p("china", "China", 52, "Oriental 18\"", "china"),
        p("splash", "Splash", 55, "A 10\"", "splash"),
    ]
}

/// The pieces from the rig: its notes, its slots' labels and instruments.
fn pieces_of(pieces: &[PieceInfo], slots: &[KitSlot]) -> Vec<Piece> {
    pieces
        .iter()
        .map(|p| {
            let slot = slots.iter().find(|s| s.slot_id == p.id);
            let label = slot.map_or_else(|| label_of(&p.id), |s| s.label.clone());
            Piece {
                id: p.id.clone(),
                family: Family::of(&p.id, &label),
                label,
                note: p.note,
                instrument: slot.map(|s| s.current_name.clone()).unwrap_or_default(),
                kind: slot.map(|s| s.kind.clone()).unwrap_or_default(),
            }
        })
        .collect()
}

#[component]
pub fn DrumTablet() -> Element {
    // Its menus (the kit, ⋯) open over it.
    signal_widgets::PopupHost::provide();
    let rig = use_hook(try_consume_context::<DrumRigClient>);
    let stream = use_hook(try_consume_context::<DrumRigStreamClient>);
    let live = rig.is_some();
    let mut kits = use_signal(Vec::<KitInfo>::new);
    let mut raw = use_signal(Vec::<PieceInfo>::new);
    let mut slots = use_signal(Vec::<KitSlot>::new);
    let mut mixer = use_signal(Vec::<MixerStrip>::new);
    // The piece picked (by its id), and the pads held down.
    let mut picked = use_signal(|| None::<String>);
    let held = use_signal(|| [false; 16]);
    // Demo mode: mute and solo kept here.
    let demo_state = use_signal(Vec::<(String, bool, bool)>::new);

    {
        let rig = rig.clone();
        use_future(move || {
            let rig = rig.clone();
            async move {
                let Some(rig) = rig else { return };
                if let Ok(k) = rig.kits().await {
                    kits.set(k);
                }
                if let Ok(p) = rig.pieces().await {
                    raw.set(p);
                }
                if let Ok(s) = rig.kit_slots().await {
                    slots.set(s);
                }
                if let Ok(m) = rig.mixer().await {
                    mixer.set(m);
                }
            }
        });
    }
    architect::use_stream(
        move |sink| {
            let stream = stream.clone();
            async move {
                match stream {
                    Some(s) => s.events(sink).await.is_ok(),
                    None => false,
                }
            }
        },
        move |ev: DrumEvent| {
            let (mut kits, mut raw, mut slots, mut mixer) = (kits, raw, slots, mixer);
            match ev {
                DrumEvent::Library(k) => kits.set(k),
                DrumEvent::Kit(p) => raw.set(p),
                DrumEvent::Design(s) => slots.set(s),
                DrumEvent::Mixer(m) => mixer.set(m),
                _ => {}
            }
        },
    );

    let pieces = if live { pieces_of(&raw.read(), &slots.read()) } else { demo_kit() };
    let pads = lay_out(&pieces);
    let kit_name = if live {
        kits.read().iter().find(|k| k.loaded).map_or_else(|| "No kit".to_string(), |k| k.name.clone())
    } else {
        "Demo Kit".to_string()
    };
    let sel = picked().and_then(|id| pieces.iter().position(|p| p.id == id)).or_else(|| (!pieces.is_empty()).then_some(0));
    // A piece's strip in the drum mixer (by its label, else its place).
    let strip_of = move |i: usize, label: &str| -> Option<MixerStrip> {
        let m = mixer.read();
        let strips: Vec<&MixerStrip> = m.iter().filter(|s| s.kind == StripKind::Piece).collect();
        strips.iter().find(|s| s.label.eq_ignore_ascii_case(label)).or_else(|| strips.get(i)).map(|s| (*s).clone())
    };
    rsx! {
        div { style: "position: relative; width: 100%; height: 100%; display: flex; flex-direction: column; background: {DESK}; color: {INK}; font-family: {FONT};",
            DrumTopBar { kit: kit_name, kits: kits.read().clone(), live }
            div { style: "flex: 1; min-height: 0; display: flex;",
                aside { style: "width: {SIDEBAR_W}px; flex-shrink: 0; display: flex; flex-direction: column; border-right: 1px solid {RULE}; background: {SHEET}; min-height: 0;",
                    PieceList { pieces: pieces.clone(), selected: sel, on_pick: move |id: String| picked.set(Some(id)) }
                    if let Some(i) = sel {
                        {
                            let p = pieces[i].clone();
                            let strip = strip_of(i, &p.label);
                            let (muted, soloed) = match &strip {
                                Some(s) => (s.muted, s.soloed),
                                None => demo_state.read().iter().find(|(id, ..)| *id == p.id).map_or((false, false), |(_, m, s)| (*m, *s)),
                            };
                            rsx! {
                                Inspector { key: "{p.id}", piece: p, strip_idx: strip.as_ref().map(|s| s.idx), muted, soloed, demo: demo_state }
                            }
                        }
                    }
                }
                main { style: "flex: 1; min-width: 0; display: flex; padding: 14px; box-sizing: border-box; background: {MAIN};",
                    PadGrid { pieces: pieces.clone(), pads, selected: sel, held, on_hit: move |i: usize| picked.set(pieces.get(i).map(|p| p.id.clone())) }
                }
            }
            signal_widgets::PopupLayer {}
        }
    }
}

/// The top bar: the instrument and its kit; ⋯ holds the way back to the
/// app's instruments (two steps, never hit by accident).
#[component]
fn DrumTopBar(kit: String, kits: Vec<KitInfo>, live: bool) -> Element {
    let rig = use_hook(try_consume_context::<DrumRigClient>);
    let host = try_use_context::<crate::phone::PhoneHost>();
    let popup = signal_widgets::PopupHost::try_use();
    let kit_items: Vec<Item> = std::iter::once(Item::head("Kits"))
        .chain(kits.iter().enumerate().map(|(i, k)| Item::run(format!("kit:{i}"), k.name.clone()).checked(k.loaded)))
        .collect();
    let on_kit = EventHandler::new(move |p: Picked| {
        if let (Some(i), Some(r)) = (p.id.strip_prefix("kit:").and_then(|i| i.parse::<u32>().ok()), rig.clone()) {
            spawn(async move {
                let _ = r.load_kit(i).await;
            });
        }
    });
    let mut more = vec![Item::head("Drums")];
    if host.is_some() {
        more.push(Item::run("instruments", "Instruments"));
    }
    let on_more = EventHandler::new(move |p: Picked| {
        if p.id == "instruments"
            && let Some(h) = host
        {
            h.on_home.call(());
        }
    });
    rsx! {
        header { style: "height: {TOP_H}px; flex-shrink: 0; display: flex; align-items: center; gap: 4px; padding: 0 6px 0 18px; border-bottom: 1px solid {RULE}; background: {SHEET}; box-sizing: border-box;",
            span { style: "font-size: 17px; font-weight: 800; letter-spacing: -0.01em;", "Drums" }
            span { style: "width: 1px; height: 24px; margin: 0 12px; background: {RULE_STRONG};" }
            button {
                style: "height: {HIT}px; display: flex; align-items: center; gap: 8px; padding: 0 12px; border: none; border-radius: {R}; background: transparent; color: {INK}; font-size: 15px; font-weight: 650; font-family: {FONT}; cursor: pointer;",
                disabled: !live || kits.is_empty(),
                onclick: move |e: MouseEvent| {
                    let (c, el) = (e.client_coordinates(), e.element_coordinates());
                    let (left, top) = (c.x - el.x, c.y - el.y);
                    super::menu::open_menu_by(popup, left, top + f64::from(TOP_H), top, kit_items.clone(), on_kit);
                },
                "{kit}"
                if live && !kits.is_empty() {
                    svg { width: "10", height: "6", view_box: "0 0 10 6",
                        path { d: "M1 1l4 4 4-4", fill: "none", stroke: INK_2, stroke_width: "1.6", stroke_linecap: "round", stroke_linejoin: "round" }
                    }
                }
            }
            span { style: "flex: 1;" }
            if !live {
                span { style: "font-size: 12px; font-weight: 700; letter-spacing: 0.08em; text-transform: uppercase; color: {INK_3}; padding: 0 12px;", "No drum engine" }
            }
            MoreButton { label: "Drum rig".to_string(), items: more, on_pick: on_more }
        }
    }
}

/// The kit's pieces by family; the picked one lifted.
#[component]
fn PieceList(pieces: Vec<Piece>, selected: Option<usize>, on_pick: EventHandler<String>) -> Element {
    rsx! {
        div { style: "flex: 1; min-height: 0; overflow-y: auto;",
            for fam in Family::ALL {
                if pieces.iter().any(|p| p.family == fam) {
                    div { key: "{fam.label()}",
                        div { style: "display: flex; align-items: center; gap: 8px; padding: 14px 18px 6px;",
                            span { style: "width: 8px; height: 8px; border-radius: 2px; background: {fam.colour()};" }
                            span { style: "font-size: 12px; font-weight: 700; letter-spacing: 0.1em; text-transform: uppercase; color: {INK_2};", "{fam.label()}" }
                        }
                        for (i, p) in pieces.iter().enumerate().filter(|(_, p)| p.family == fam) {
                            {
                                let on = selected == Some(i);
                                let id = p.id.clone();
                                rsx! {
                                    button {
                                        key: "{p.id}",
                                        style: "width: 100%; min-height: 56px; display: flex; align-items: center; gap: 12px; padding: 6px 18px; border: none; border-bottom: 1px solid {RULE}; background: {pick(on, UP, CLEAR)}; color: {INK}; text-align: left; font-family: {FONT}; cursor: pointer;",
                                        onclick: move |_| on_pick.call(id.clone()),
                                        span { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px;",
                                            span { style: "font-size: 16px; font-weight: {pick(on, 750, 560)}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{p.label}" }
                                            if !p.instrument.is_empty() {
                                                span { style: "font-size: 13px; color: {INK_3}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{p.instrument}" }
                                            }
                                        }
                                        span { style: "font-size: 13px; font-weight: 650; color: {INK_3}; font-variant-numeric: tabular-nums;", "{p.note}" }
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

/// The picked piece: its instrument, stepped through the ones like it
/// (the rig's similarity space, else the library's of its kind), and its
/// mute and solo.
#[component]
fn Inspector(piece: Piece, strip_idx: Option<u32>, muted: bool, soloed: bool, demo: Signal<Vec<(String, bool, bool)>>) -> Element {
    let rig = use_hook(try_consume_context::<DrumRigClient>);
    let mut options = use_signal(Vec::<LibraryPiece>::new);
    {
        let (rig, id, kind) = (rig.clone(), piece.id.clone(), piece.kind.clone());
        use_future(move || {
            let (rig, id, kind) = (rig.clone(), id.clone(), kind.clone());
            async move {
                let Some(r) = rig else { return };
                let mut list = r.similar_pieces(id).await.unwrap_or_default();
                if list.is_empty() {
                    list = r.library().await.unwrap_or_default().into_iter().filter(|l| l.kind.eq_ignore_ascii_case(&kind)).collect();
                }
                options.set(list);
            }
        });
    }
    let at = options.read().iter().position(|o| o.name == piece.instrument);
    let step = {
        let rig = rig.clone();
        let id = piece.id.clone();
        move |d: i32| {
            let list = options.read().clone();
            if list.is_empty() {
                return;
            }
            let n = list.len() as i32;
            let next = ((at.map_or(-1, |a| a as i32) + d).rem_euclid(n)) as usize;
            if let (Some(r), Some(o)) = (rig.clone(), list.get(next).cloned()) {
                let id = id.clone();
                spawn(async move {
                    let _ = r.swap_piece(id, o.path).await;
                });
            }
        }
    };
    let set = {
        let rig = rig.clone();
        let id = piece.id.clone();
        move |mute: bool, on: bool| {
            match (rig.clone(), strip_idx) {
                (Some(r), Some(idx)) => {
                    spawn(async move {
                        let _ = if mute { r.set_piece_mute(idx, on).await } else { r.set_piece_solo(idx, on).await };
                    });
                }
                _ => {
                    let mut demo = demo;
                    let mut d = demo.write();
                    match d.iter_mut().find(|(i, ..)| *i == id) {
                        Some(e) => {
                            if mute {
                                e.1 = on;
                            } else {
                                e.2 = on;
                            }
                        }
                        None => d.push((id.clone(), mute && on, !mute && on)),
                    }
                }
            }
        }
    };
    let (s1, s2) = (step.clone(), step);
    let (m1, m2) = (set.clone(), set);
    let count = options.read().len();
    let toggle = |on: bool, colour: &str| {
        format!(
            "flex: 1; height: {HIT}px; border-radius: {R}; border: 1px solid {}; background: {}; color: {}; font-size: 15px; font-weight: 700; font-family: {FONT}; cursor: pointer;",
            if on { colour } else { RULE_STRONG },
            if on { colour } else { CLEAR },
            if on { "#0a0a0c" } else { INK_2 }
        )
    };
    rsx! {
        div { style: "flex-shrink: 0; border-top: 1px solid {RULE_STRONG}; background: {SHEET_2}; padding: 14px 18px 18px; display: flex; flex-direction: column; gap: 12px;",
            div { style: "display: flex; align-items: center; gap: 10px;",
                span { style: "width: 10px; height: 10px; border-radius: 2px; background: {piece.family.colour()};" }
                span { style: "flex: 1; min-width: 0; font-size: 22px; font-weight: 800; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{piece.label}" }
                span { style: "font-size: 13px; font-weight: 650; color: {INK_3}; font-variant-numeric: tabular-nums;", "Note {piece.note}" }
            }
            // The instrument, stepped through its like.
            div { style: "display: flex; align-items: center; gap: 4px; height: {HIT}px; border-radius: {R}; background: {FILL};",
                button { "aria-label": "Previous instrument", disabled: count == 0, style: "width: {HIT}px; height: {HIT}px; display: flex; align-items: center; justify-content: center; border: none; background: transparent; cursor: pointer;", onclick: move |_| s1(-1),
                    super::fx_row::Chevron { left: true }
                }
                span { style: "flex: 1; min-width: 0; text-align: center; font-size: 15px; font-weight: 650; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; color: {pick(piece.instrument.is_empty(), INK_3, INK)};",
                    if piece.instrument.is_empty() { "—" } else { "{piece.instrument}" }
                }
                button { "aria-label": "Next instrument", disabled: count == 0, style: "width: {HIT}px; height: {HIT}px; display: flex; align-items: center; justify-content: center; border: none; background: transparent; cursor: pointer;", onclick: move |_| s2(1),
                    super::fx_row::Chevron { left: false }
                }
            }
            div { style: "display: flex; gap: 8px;",
                button { style: "{toggle(muted, VOID)}", onclick: move |_| m1(true, !muted), "Mute" }
                button { style: "{toggle(soloed, \"#facc15\")}", onclick: move |_| m2(false, !soloed), "Solo" }
            }
        }
    }
}

/// The 4×4 pads, pad 1 at the bottom left. A pad plays as a finger lands —
/// harder the higher on the pad — and lets go as it lifts.
#[component]
fn PadGrid(pieces: Vec<Piece>, pads: [Option<usize>; 16], selected: Option<usize>, held: Signal<[bool; 16]>, on_hit: EventHandler<usize>) -> Element {
    // Rows top to bottom: pads 13–16, 9–12, 5–8, 1–4.
    let order: Vec<usize> = (0..4).rev().flat_map(|r| (0..4).map(move |c| r * 4 + c)).collect();
    rsx! {
        div { style: "flex: 1; min-width: 0; display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); grid-template-rows: repeat(4, minmax(0, 1fr)); gap: 10px;",
            for pad in order {
                {
                    let piece = pads[pad].and_then(|i| pieces.get(i).cloned().map(|p| (i, p)));
                    rsx! {
                        Pad { key: "{pad}", no: pad as u32 + 1, piece, selected, held, on_hit }
                    }
                }
            }
        }
    }
}

#[component]
fn Pad(no: u32, piece: Option<(usize, Piece)>, selected: Option<usize>, held: Signal<[bool; 16]>, on_hit: EventHandler<usize>) -> Element {
    let rig = use_hook(try_consume_context::<DrumRigClient>);
    // The pad's height, measured: where on it a finger lands is how hard.
    let mut pad_h = use_signal(|| 160.0_f64);
    let slot = (no - 1) as usize;
    let Some((i, p)) = piece else {
        return rsx! {
            div { style: "border-radius: {R_MD}; border: 1px dashed {RULE_STRONG}; display: flex; align-items: flex-start; padding: 10px 12px; box-sizing: border-box;",
                span { style: "font-size: 12px; font-weight: 650; color: {DIM};", "{no}" }
            }
        };
    };
    let down = held.read()[slot];
    let on = selected == Some(i);
    let colour = p.family.colour();
    let note = p.note;
    let (r_down, r_up) = (rig.clone(), rig);
    let release = move || {
        let mut held = held;
        if held.peek()[slot] {
            held.write()[slot] = false;
            if let Some(r) = r_up.clone() {
                spawn(async move {
                    let _ = r.trigger(note, 0).await;
                });
            }
        }
    };
    let (rel1, rel2, rel3) = (release.clone(), release.clone(), release);
    let bg = if down { tint(colour, 55) } else { tint(colour, 12) };
    let edge = if on { format!("2px solid {FOCUS_FG}") } else { format!("1px solid {RULE_STRONG}") };
    rsx! {
        div {
            "aria-label": "Pad {no}: {p.label}",
            role: "button",
            onmounted: move |e| {
                let el = e.data();
                spawn(async move {
                    if let Ok(r) = el.get_client_rect().await
                        && r.height() > 1.0
                    {
                        pad_h.set(r.height());
                    }
                });
            },
            style: "position: relative; border-radius: {R_MD}; border: {edge}; background: {bg}; overflow: hidden; display: flex; flex-direction: column; justify-content: space-between; padding: 12px 14px; box-sizing: border-box; touch-action: none; user-select: none; cursor: pointer;",
            onpointerdown: move |e: PointerEvent| {
                e.prevent_default();
                // Higher on the pad, harder: 127 at the top, 40 at the foot.
                let y = e.element_coordinates().y;
                let h = pad_h.peek().max(1.0);
                let vel = (127.0 - (y / h).clamp(0.0, 1.0) * 87.0).round() as u32;
                let mut held = held;
                held.write()[slot] = true;
                on_hit.call(i);
                if let Some(r) = r_down.clone() {
                    spawn(async move {
                        let _ = r.trigger(note, vel.clamp(1, 127)).await;
                    });
                }
            },
            onpointerup: move |_| rel1(),
            onpointercancel: move |_| rel2(),
            onpointerleave: move |_| rel3(),
            // The family's band along the top.
            span { style: "position: absolute; left: 0; right: 0; top: 0; height: 6px; background: {colour};" }
            div { style: "display: flex; justify-content: space-between; align-items: baseline; margin-top: 4px;",
                span { style: "font-size: 12px; font-weight: 700; color: {INK_3}; font-variant-numeric: tabular-nums;", "{no}" }
                span { style: "font-size: 12px; font-weight: 650; color: {INK_3}; font-variant-numeric: tabular-nums;", "{p.note}" }
            }
            div { style: "display: flex; flex-direction: column; gap: 2px; min-width: 0;",
                span { style: "font-size: 19px; font-weight: 800; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{p.label}" }
                if !p.instrument.is_empty() {
                    span { style: "font-size: 13px; color: {INK_2}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{p.instrument}" }
                }
            }
        }
    }
}
