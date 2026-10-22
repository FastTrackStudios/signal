//! The switches — the prototype's `dock/Switches.tsx` on the rig: the
//! footswitch grid, flush, two rows of five.
//!
//!   row A   switches 6–10, the hold layer (a foot's hold lives "up" from
//!           the toe): Ambient · FX Toggle · Song · Boost · Tuner
//!   row B   switches 1–5: the profile's first four stacks, then Tap Tempo
//!           — or, where switch 5 is a stack (the bass's), the fifth
//!
//! Compact (under a gig's chart): the same two rows and columns, both cut
//! down, so all ten switches stay in reach.
//!
//! Gig (the performer screen's rig dock, worship / gig screen spec §4.5):
//! two 60 px rows of five tiles, 8 px apart, rounded — at the touch floor
//! and above it, beside the transport.
//!
//! Every lit tile's words are white or near-black, whichever reads better
//! on its colour ([`ink_on`]): white on every colour left Lead, FX, Song
//! and Ambient under 4.5:1.
//!
//! A stack switch plays its stack; pressed again it steps through it. Its
//! tile is the stack's colour, lit when it plays, dimmed toward the grid
//! when not.

use crate::proto::rig::RigClient;
use crate::proto::{PerfStack, PerformanceModel};
use dioxus::prelude::*;

use super::tokens::*;

/// A stack's tile and its text — `perform::folder_color`, the one table.
fn folder(name: &str) -> (&'static str, &'static str) {
    crate::ui::perform::folder_color(name)
}

/// The switches' light text.
const WHITE: &str = "#fafafa";
/// The switches' dark text, for the light colours.
const BLACK: &str = "#0a0a0a";
/// A Gig tile's words while it isn't playing (6.6:1 and up on a dimmed
/// tile).
const IDLE_INK: &str = "#d4d4d8";

/// How big the switches are drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SwitchSize {
    /// As tall as the FX row it swaps with.
    #[default]
    Full,
    /// Cut down under a chart (38 + 58).
    Compact,
    /// The performer screen's dock: 60 + 60, 8 px gaps.
    Gig,
}

/// A Gig row's height and the gap between tiles, px.
pub const GIG_ROW: f64 = 60.0;
pub const GIG_GAP: f64 = 8.0;

/// The ten switches' lit colours: (switch, tile, its words).
pub const SWITCH_COLOURS: [(&str, &str); 10] = [
    ("Clean", "#38bdf8"),
    ("Crunch", "#2563eb"),
    ("Drive", "#f97316"),
    ("Lead", "#ef4444"),
    ("Ambient", "#06b6d4"),
    ("FX Toggle", FX_BG),
    ("Song", SONG_BG),
    ("Boost", BOOST_BG),
    ("Tuner", TUNER_BG),
    ("Tap Tempo", TAP_BG),
];
const FX_BG: &str = "#ec4899";
const SONG_BG: &str = "#a78bfa";
const BOOST_BG: &str = "#fafafa";
const TUNER_BG: &str = "#141418";
/// The tuner's tile when the string is in tune, and its words.
const TUNED_BG: &str = "#14532d";
const TUNED_INK: &str = "#4ade80";

/// A colour's channels, 0..255, from `#rrggbb` or `rgb(r,g,b)`.
fn channels(colour: &str) -> [f64; 3] {
    let c = colour.trim();
    if let Some(body) = c.strip_prefix("rgb(").and_then(|b| b.strip_suffix(')')) {
        let mut out = [0.0; 3];
        for (slot, v) in out.iter_mut().zip(body.split(',')) {
            *slot = v.trim().parse().unwrap_or(0.0);
        }
        return out;
    }
    let ch = |i: usize| f64::from(u8::from_str_radix(c.get(1 + i..3 + i).unwrap_or("00"), 16).unwrap_or(0));
    [ch(0), ch(2), ch(4)]
}

/// WCAG relative luminance.
#[must_use]
pub fn luminance(colour: &str) -> f64 {
    let lin = |v: f64| {
        let v = v / 255.0;
        if v <= 0.039_28 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    let [r, g, b] = channels(colour);
    0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

/// The WCAG contrast ratio of two colours (1..21).
#[must_use]
pub fn contrast(a: &str, b: &str) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// The words for a tile of this colour: white or near-black, whichever
/// reads better.
#[must_use]
pub fn ink_on(bg: &str) -> &'static str {
    if contrast(WHITE, bg) >= contrast(BLACK, bg) { WHITE } else { BLACK }
}

/// switches::dim — a colour darkened toward the grid's ground.
fn dim(hex: &str, amount: f64) -> String {
    let ch = |i: usize| {
        f64::from(u8::from_str_radix(hex.get(1 + i..3 + i).unwrap_or("00"), 16).unwrap_or(0))
    };
    let base = [10.0, 10.0, 12.0];
    let c: Vec<String> = [0usize, 2, 4]
        .iter()
        .enumerate()
        .map(|(k, &i)| format!("{}", (base[k] + (ch(i) - base[k]) * amount).round() as i32))
        .collect();
    format!("rgb({})", c.join(","))
}

const ROW: &str = "row";
const TRANSPARENT: &str = "transparent";
const DOCK_BG: &str = "#0a0a0c";
/// A Gig tile's padding: room on top for the number and the caption.
const GIG_PAD: &str = "14px 8px 0";
const GIG_PAD_WIDE: &str = "14px 10px 0";
const FLUSH_PAD: &str = "0 8px";
const FLUSH_PAD_WIDE: &str = "0 10px";
const NO_PAD: &str = "0";
const COLUMN: &str = "column";
const TAP_FLASH: &str = "#52525b";
const TAP_BG: &str = "#3f3f46";

/// The lit switch's ring, inside the tile (flush tiles have no room
/// outside) — an overlay with a border: Blitz draws no inset shadow.
#[component]
fn LitRing(#[props(default)] radius: u32) -> Element {
    rsx! { span { style: "position: absolute; left: 0; top: 0; right: 0; bottom: 0; border: 2px solid rgba(255,255,255,0.85); border-radius: {radius}px; box-sizing: border-box; pointer-events: none;" } }
}

/// The dock's two rows: the hold layer a third of it, the switches under
/// the feet two thirds — of the FX row's height, less the hairline between
/// and the border on top.
const HOLD_H: f64 = (super::fx_row::FX_H - 2.0) / 3.0;
const MAIN_H: f64 = (super::fx_row::FX_H - 2.0) * 2.0 / 3.0;
/// Compact: both rows cut down (the hold layer over the switches under
/// the feet), well under the FX row's height.
const COMPACT_HOLD_H: f64 = 38.0;
const COMPACT_H: f64 = 58.0;

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

#[component]
pub fn TouchSwitches(
    perf: PerformanceModel,
    /// Both rows (6–10 over 1–5), cut down: a chart above gets the room.
    /// The same as `size: SwitchSize::Compact`.
    #[props(default)]
    compact: bool,
    /// How big: Full (the default), Compact, or Gig (the performer
    /// screen's dock).
    #[props(default)]
    size: SwitchSize,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let stacks = perf.stacks.clone();
    let song = perf
        .songs
        .get(perf.song_index as usize)
        .map(|s| s.name.clone())
        .unwrap_or_else(|| "—".to_string());
    let fx = !perf.fx_bypass;
    let boost = perf.boost_db.abs() > 0.01;
    // Off: the level it switches on at, as the rig keeps it.
    let boost_label = format!(
        "{:+.0} dB",
        if boost {
            perf.boost_db
        } else {
            perf.boost_level
        }
    );
    // Switch 5 a stack (the bass's fifth): its tile under the foot, and the
    // hold layer's first tile the sixth stack.
    let five_is_stack =
        perf.switch_actions.get(4).is_some_and(|a| a == "stack") && stacks.len() > 4;
    // Full: as tall as the FX row. Compact: the same ten switches, the
    // same columns, both rows cut down — a chart above gets the room.
    let size = if compact && size == SwitchSize::Full {
        SwitchSize::Compact
    } else {
        size
    };
    let gig = size == SwitchSize::Gig;
    let compact = size == SwitchSize::Compact;
    let (hold_h, main_h) = match size {
        SwitchSize::Full => (HOLD_H, MAIN_H),
        SwitchSize::Compact => (COMPACT_HOLD_H, COMPACT_H),
        SwitchSize::Gig => (GIG_ROW, GIG_ROW),
    };
    // Flush tiles: a hairline between and one on top. Gig: 8 px apart.
    let (gap, grid_h, ground, top) = if gig {
        (GIG_GAP, hold_h + main_h + GIG_GAP, "transparent", "none")
    } else {
        (1.0, hold_h + main_h + 2.0, "#000", "1px solid #000")
    };
    rsx! {
        div { style: "display: flex; flex-direction: column; background: {pick(gig, TRANSPARENT, DOCK_BG)};",
            // One grid, two rows of five, the same columns: the hold layer
            // slim above, the switches under the feet tall below. Flush: a
            // hairline of the ground between.
            // As tall as the FX row it swaps with (a landscape iPhone's
            // room): the grid above never moves.
            div { style: "display: grid; grid-template-columns: repeat(5, minmax(0, 1fr)); grid-template-rows: {hold_h}px {main_h}px; gap: {gap}px; background: {ground}; border-top: {top}; box-sizing: border-box; height: {grid_h}px;",
                if let Some((i, st)) = { let i = if five_is_stack { 5 } else { 4 }; stacks.get(i).cloned().map(|s| (i, s)) } {
                    StackTile { no: 6, stack: st, index: i, compact: !gig, gig }
                } else {
                    Empty { no: 6, gig }
                }
                FnTile { no: 7, title: "FX Toggle", subtitle: if fx { "Time FX on".to_string() } else { "Time FX off".to_string() }, bg: FX_BG, lit: fx, gig,
                    onclick: { let rig = rig.clone(); move |_| call!(rig, |r| r.toggle_fx()) } }
                FnTile { no: 8, title: "Song", subtitle: song, bg: SONG_BG, lit: perf.perform_mode == 2, gig,
                    onclick: { let rig = rig.clone(); move |_| call!(rig, |r| r.next_song()) } }
                FnTile { no: 9, title: "Boost", subtitle: if boost { boost_label.clone() } else { format!("{boost_label} · off") }, bg: BOOST_BG, lit: boost, gig,
                    onclick: { let rig = rig.clone(); move |_| call!(rig, |r| r.toggle_boost()) } }
                // Always tuning; a press mutes the guitar into the chain to
                // tune in silence.
                TunerTile { muted: perf.tuner_visible, gig, onclick: { let rig = rig.clone(); move |_| call!(rig, |r| r.toggle_tuner()) } }
                for i in 0..4usize {
                    if let Some(st) = stacks.get(i).cloned() {
                        StackTile { key: "m{i}", no: i as u32 + 1, stack: st, index: i, compact, gig }
                    } else {
                        Empty { key: "m{i}", no: i as u32 + 1, gig }
                    }
                }
                if five_is_stack {
                    StackTile { no: 5, stack: stacks[4].clone(), index: 4, compact, gig }
                } else {
                    TapTempo { bpm: perf.tempo_bpm, compact, gig }
                }
            }
        }
    }
}

/// A stack's switch: lit in its folder colour when it plays, dark when not.
/// The part it plays is the big name (the patch, by its own name); the
/// stack's name small at the top; the preset the part loads, with what it
/// overrides, under the name; dots say where the next press lands.
#[component]
fn StackTile(
    no: u32,
    stack: PerfStack,
    index: usize,
    compact: bool,
    #[props(default)] gig: bool,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let (bg, _) = folder(&stack.name);
    let lit = stack.is_active;
    // The part a press plays: its patch, by its own name.
    let showing = stack
        .patches
        .get(stack.position as usize)
        .cloned()
        .unwrap_or_else(|| stack.current_patch.clone());
    let part = if showing.is_empty() {
        stack.name.clone()
    } else {
        showing
    };
    let count = stack.patches.len().max(stack.patch_count as usize);
    let pos = stack.position as usize;
    let tile_bg = if lit { bg.to_string() } else { dim(bg, 0.24) };
    // The better of white and near-black on the stack's colour; muted
    // while another plays.
    let ink = match (lit, gig) {
        (true, _) => ink_on(bg).to_string(),
        (false, true) => IDLE_INK.to_string(),
        (false, false) => dim(WHITE, 0.5),
    };
    let dot = if compact || gig { 5 } else { 6 };
    let radius = if gig { 8 } else { 0 };
    let direction = pick(compact && !gig, ROW, COLUMN);
    let name_size = if gig { 18.0 } else { name_px(&part, compact) };
    // Gig: the stack's name over the part, under the switch's number.
    let stack_at = if gig { "left: 26px;" } else { "left: 24px;" };
    // A momentary switch plays while held: pressed on the way down,
    // released on the way up.
    let momentary = stack.momentary;
    let mut down = use_signal(|| false);
    let (r_click, r_down, r_up, r_leave, r_cancel) =
        (rig.clone(), rig.clone(), rig.clone(), rig.clone(), rig);
    rsx! {
        button {
            "aria-pressed": "{lit}",
            "aria-label": "Switch {no}, {stack.name}: {part}",
            style: "position: relative; min-width: 0; overflow: hidden; border: none; border-radius: {radius}px; padding: {pick(gig, GIG_PAD, FLUSH_PAD)}; box-sizing: border-box; display: flex; flex-direction: {direction}; align-items: center; justify-content: center; gap: {pick(compact && !gig, 8, pick(gig, 2, 4))}px; background: {tile_bg}; color: {ink}; font-family: {FONT}; cursor: pointer;",
            onclick: move |_| if !momentary { call!(r_click, |r| r.press_stack(index as u32)) },
            onpointerdown: move |_| if momentary {
                down.set(true);
                call!(r_down, |r| r.press_stack(index as u32));
            },
            onpointerup: move |_| if momentary && down() {
                down.set(false);
                call!(r_up, |r| r.release_stack(index as u32));
            },
            onpointerleave: move |_| if momentary && down() {
                down.set(false);
                call!(r_leave, |r| r.release_stack(index as u32));
            },
            onpointercancel: move |_| if momentary && down() {
                down.set(false);
                call!(r_cancel, |r| r.release_stack(index as u32));
            },
            if lit { LitRing { radius } }
            SwitchNo { no, ink: ink.clone() }
            // The stack, small, beside the switch's number.
            span { style: "position: absolute; top: 5px; {stack_at} font-size: 12px; font-weight: 800; letter-spacing: 0.08em; text-transform: uppercase; color: {ink}; opacity: 0.85;", "{stack.name}" }
            span { style: "font-size: {name_size}px; font-weight: 750; letter-spacing: 0.01em; white-space: nowrap; overflow: hidden; max-width: 100%; color: {ink};", "{part}" }
            // One grey mark at the top right when it overrides anything.
            if !stack.override_modules.is_empty() {
                span { style: "position: absolute; top: 6px; right: 8px; display: flex;",
                    super::marks::OverrideIcon { colour: INK_2.to_string(), size: if compact { 10 } else { 12 } }
                }
            }
            // The preset it loads, under its name.
            if !stack.preset.is_empty() && !compact && !gig {
                span { style: "font-size: 13px; font-weight: 600; opacity: 0.85; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 100%; color: {ink};", "{core_line(&stack.preset)}" }
            }
            if count > 1 {
                span { style: "display: flex; align-items: center; gap: {pick(compact || gig, 4, 6)}px; margin-top: {pick(compact || gig, 0, 4)}px;",
                    for k in 0..count {
                        span { key: "{k}", style: "width: {dot}px; height: {dot}px; border-radius: 999px; background: {ink}; opacity: {pick(k == pos, 0.95, 0.35)};" }
                    }
                }
            }
        }
    }
}

/// A function switch: a title and what it is doing, lit in its colour.
#[component]
fn FnTile(
    no: u32,
    title: &'static str,
    subtitle: String,
    bg: &'static str,
    lit: bool,
    #[props(default)] gig: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let tile_bg = if lit { bg.to_string() } else { dim(bg, 0.3) };
    // The better of white and near-black on its colour.
    let text = ink_on(bg);
    let ink = match (lit, gig) {
        (true, _) => text.to_string(),
        (false, true) => IDLE_INK.to_string(),
        (false, false) => dim(text, 0.45),
    };
    if gig {
        // The title small over the switch's number line, what it is doing
        // big under it.
        return rsx! {
            button {
                "aria-pressed": "{lit}",
                "aria-label": "Switch {no}, {title}: {subtitle}",
                style: "position: relative; min-width: 0; overflow: hidden; border: none; border-radius: 8px; padding: 14px 8px 0; box-sizing: border-box; display: flex; flex-direction: column; align-items: center; justify-content: center; background: {tile_bg}; color: {ink}; font-family: {FONT}; cursor: pointer;",
                onclick: move |e| onclick.call(e),
                if lit { LitRing { radius: 8 } }
                SwitchNo { no, ink: ink.clone() }
                span { style: "position: absolute; top: 5px; left: 26px; font-size: 12px; font-weight: 800; letter-spacing: 0.08em; text-transform: uppercase; white-space: nowrap; color: {ink}; opacity: 0.85;", "{title}" }
                span { style: "font-size: 18px; font-weight: 750; white-space: nowrap; overflow: hidden; max-width: 100%; color: {ink};", "{subtitle}" }
            }
        };
    }
    rsx! {
        button {
            "aria-pressed": "{lit}",
            "aria-label": "Switch {no}, {title}: {subtitle}",
            style: "position: relative; min-width: 0; overflow: hidden; border: none; border-radius: 0; padding: 0 8px; display: flex; flex-direction: row; align-items: center; justify-content: center; gap: 8px; background: {tile_bg}; color: {ink}; font-family: {FONT}; cursor: pointer;",
            onclick: move |e| onclick.call(e),
            if lit { LitRing {} }
            SwitchNo { no, ink: ink.clone() }
            span { style: "font-size: 14px; font-weight: 700; letter-spacing: 0.02em; white-space: nowrap; color: {ink};", "{title}" }
            span { style: "font-size: 13px; opacity: 0.85; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; color: {ink};", "{subtitle}" }
        }
    }
}

#[component]
fn Empty(no: u32, #[props(default)] gig: bool) -> Element {
    rsx! {
        div { style: "position: relative; background: #0e0e11; border-radius: {pick(gig, 8, 0)}px;",
            SwitchNo { no, ink: INK.to_string() }
        }
    }
}

#[component]
fn TapTempo(
    bpm: u32,
    #[props(default)] compact: bool,
    #[props(default)] gig: bool,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut flash = use_signal(|| false);
    let tempo = if bpm == 0 { 120 } else { bpm };
    // The ring: lit on every beat at the rig's tempo, out a short while
    // after.
    let mut beat = use_signal(|| false);
    let mut period = use_signal(|| 500u64);
    let ms = 60_000 / u64::from(tempo.max(1));
    if *period.peek() != ms {
        period.set(ms);
    }
    use_future(move || async move {
        loop {
            beat.set(true);
            architect::platform::sleep(std::time::Duration::from_millis(90)).await;
            beat.set(false);
            let rest = period.peek().saturating_sub(90).max(10);
            architect::platform::sleep(std::time::Duration::from_millis(rest)).await;
        }
    });
    rsx! {
        button {
            "aria-label": "Tap tempo, {tempo} BPM",
            style: "position: relative; border: none; border-radius: {pick(gig, 8, 0)}px; padding: {pick(gig, GIG_PAD, NO_PAD)}; box-sizing: border-box; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 2px; background: {pick(flash(), TAP_FLASH, TAP_BG)}; color: #e4e4e7; font-family: {FONT}; cursor: pointer;",
            onclick: move |_| {
                call!(rig, |r| r.tap_tempo());
                flash.set(true);
                spawn(async move {
                    architect::platform::sleep(std::time::Duration::from_millis(90)).await;
                    flash.set(false);
                });
            },
            if beat() { LitRing { radius: pick(gig, 8, 0) } }
            SwitchNo { no: 5, ink: INK.to_string() }
            if gig {
                span { style: "position: absolute; top: 5px; left: 26px; font-size: 12px; font-weight: 800; letter-spacing: 0.08em; opacity: 0.85;", "TAP" }
                span { style: "font-size: 18px; font-weight: 750; font-variant-numeric: tabular-nums;", "{tempo} bpm" }
            } else {
                span { style: "font-size: {pick(compact, 16, 20)}px; font-weight: 700;", "Tap Tempo" }
                span { style: "font-size: {pick(compact, 12, 14)}px; font-weight: 600; opacity: 0.8; font-variant-numeric: tabular-nums;", "{tempo} BPM" }
            }
        }
    }
}

/// The tuner switch, always tuning: the note and how far off, the needle
/// about the middle, green when in tune. A press mutes the guitar into the
/// chain (`muted`: lit, "Mute") to tune in silence.
#[component]
fn TunerTile(
    muted: bool,
    #[props(default)] gig: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut reading = use_signal(crate::proto::TunerReading::default);
    use_future(move || {
        let rig = rig.clone();
        async move {
            let Some(rig) = rig else { return };
            loop {
                if let Ok(r) = rig.tuner().await {
                    reading.set(r);
                }
                architect::platform::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    });
    let r = reading();
    let in_tune = r.active && r.cents.abs() <= 5.0;
    let (bg, ink, needle) = if in_tune {
        (TUNED_BG, TUNED_INK, TUNED_INK)
    } else {
        (TUNER_BG, INK, "#facc15")
    };
    let at = 50.0 + r.cents.clamp(-50.0, 50.0);
    let note = if r.active {
        r.note.clone()
    } else {
        "—".to_string()
    };
    let cents = if r.active {
        format!("{:+.0}", r.cents)
    } else {
        String::new()
    };
    rsx! {
        button {
            "aria-label": if muted { "Tuner, muted" } else { "Tuner" },
            "aria-pressed": "{muted}",
            style: "position: relative; min-width: 0; overflow: hidden; border: none; border-radius: {pick(gig, 8, 0)}px; padding: {pick(gig, GIG_PAD_WIDE, FLUSH_PAD_WIDE)}; box-sizing: border-box; display: flex; flex-direction: row; align-items: center; gap: 10px; background: {bg}; color: {ink}; font-family: {FONT}; cursor: pointer;",
            onclick: move |e| onclick.call(e),
            if muted { LitRing { radius: pick(gig, 8, 0) } }
            SwitchNo { no: 10, ink: ink.to_string() }
            if muted {
                span { style: "position: absolute; top: 5px; right: 9px; font-size: 12px; font-weight: 800; letter-spacing: 0.08em; color: {ink}; opacity: 0.85;", "MUTE" }
            }
            span { style: "font-size: 17px; font-weight: 800; min-width: 34px; color: {ink};", "{note}" }
            div { style: "position: relative; flex: 1; height: 16px;",
                div { style: "position: absolute; left: 0; right: 0; top: 7px; height: 2px; background: rgba(255,255,255,0.18);" }
                div { style: "position: absolute; left: 50%; top: 0; bottom: 0; width: 2px; margin-left: -1px; background: rgba(255,255,255,0.5);" }
                if r.active {
                    div { style: "position: absolute; left: {at}%; top: 0; bottom: 0; width: 4px; margin-left: -2px; border-radius: 2px; background: {needle};" }
                }
            }
            span { style: "font-size: 12px; font-weight: 700; min-width: 24px; text-align: right; font-variant-numeric: tabular-nums; opacity: 0.8;", "{cents}" }
        }
    }
}

#[component]
fn SwitchNo(no: u32, ink: String) -> Element {
    rsx! {
        span { style: "position: absolute; top: 5px; left: 9px; font-size: 12px; font-weight: 650; font-variant-numeric: tabular-nums; opacity: 0.6; color: {ink};", "{no}" }
    }
}

/// A tile's preset line: the Core preset and its variation, without the
/// picks added on top ("Deluxe + AC30 · Clean + Amp EQ" → "Deluxe + AC30 ·
/// Clean") — the tile has room for the one thing it loads.
fn core_line(preset: &str) -> &str {
    match preset.find(" · ") {
        Some(dot) => preset[dot..]
            .find(" + ")
            .map_or(preset, |plus| &preset[..dot + plus]),
        None => preset,
    }
}

/// The part name's size on a tile: 25 pt, down to 15 for a long name, so
/// the whole name fits the tile's ~154 pt (Inter bold runs ~0.58 em a
/// letter).
fn name_px(name: &str, compact: bool) -> f64 {
    if compact {
        return 15.0;
    }
    let letters = name.chars().count().max(1) as f64;
    (154.0 / (letters * 0.58)).clamp(15.0, 25.0).floor()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG AA for text.
    const AA: f64 = 4.5;

    #[test]
    fn every_switch_colour_reads_at_4_5_to_1() {
        for (name, bg) in SWITCH_COLOURS {
            // The tuner and Tap Tempo keep their light ink.
            let ink = match name {
                "Tuner" | "Tap Tempo" => INK,
                _ => ink_on(bg),
            };
            let c = contrast(ink, bg);
            assert!(c >= AA, "{name}: {ink} on {bg} is {c:.2}:1");
        }
        let tuned = contrast(TUNED_INK, TUNED_BG);
        assert!(tuned >= AA, "tuner in tune: {tuned:.2}:1");
    }

    #[test]
    fn the_stack_colours_are_the_folder_table() {
        for name in ["Clean", "Crunch", "Drive", "Lead", "Ambient"] {
            let (bg, _) = folder(name);
            let listed = SWITCH_COLOURS.iter().find(|(n, _)| *n == name).map(|(_, c)| *c);
            assert_eq!(listed, Some(bg), "{name}");
        }
    }

    #[test]
    fn white_ink_failed_where_the_spec_said() {
        // The audit (spec §9): white on these was under 4.5:1.
        for bg in ["#ef4444", "#06b6d4", FX_BG, SONG_BG] {
            assert!(contrast(WHITE, bg) < AA, "{bg}");
            assert_eq!(ink_on(bg), BLACK, "{bg} takes the dark ink");
        }
        assert_eq!(ink_on("#2563eb"), WHITE);
    }

    #[test]
    fn idle_gig_tiles_read_too() {
        for (name, bg) in SWITCH_COLOURS {
            let idle = dim(bg, 0.24);
            let c = contrast(IDLE_INK, &idle);
            assert!(c >= AA, "{name} idle: {c:.2}:1 on {idle}");
        }
    }

    #[test]
    fn contrast_is_the_wcag_ratio() {
        assert!((contrast("#000000", "#ffffff") - 21.0).abs() < 1e-9);
        assert!((contrast("#777777", "#777777") - 1.0).abs() < 1e-9);
        assert!((luminance("rgb(255,255,255)") - 1.0).abs() < 1e-9);
    }
}
