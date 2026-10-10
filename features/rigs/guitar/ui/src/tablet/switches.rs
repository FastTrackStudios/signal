//! The switches — the prototype's `dock/Switches.tsx` on the rig: the
//! footswitch grid, flush, two rows of five.
//!
//!   row A   switches 6–10, the hold layer (a foot's hold lives "up" from
//!           the toe): Ambient · FX Toggle · Song · Boost · Tuner
//!   row B   switches 1–5: the profile's first four stacks, then Tap Tempo
//!           — or, where switch 5 is a stack (the bass's), the fifth
//!
//! A stack switch plays its stack; pressed again it steps through it. Its
//! tile is the stack's colour, lit when it plays, dimmed toward the grid
//! when not.

use dioxus::prelude::*;
use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{PerfStack, PerformanceModel};

use super::tokens::*;

/// A stack's tile and its text — `perform::folder_color`, the one table.
fn folder(name: &str) -> (&'static str, &'static str) {
    crate::perform::folder_color(name)
}

/// The switches' text: white on every colour (Boost, on white, keeps black).
const WHITE: &str = "#fafafa";

/// switches::dim — a colour darkened toward the grid's ground.
fn dim(hex: &str, amount: f64) -> String {
    let ch = |i: usize| f64::from(u8::from_str_radix(hex.get(1 + i..3 + i).unwrap_or("00"), 16).unwrap_or(0));
    let base = [10.0, 10.0, 12.0];
    let c: Vec<String> = [0usize, 2, 4]
        .iter()
        .enumerate()
        .map(|(k, &i)| format!("{}", (base[k] + (ch(i) - base[k]) * amount).round() as i32))
        .collect();
    format!("rgb({})", c.join(","))
}

const ROW: &str = "row";
const COLUMN: &str = "column";
const TAP_FLASH: &str = "#52525b";
const TAP_BG: &str = "#3f3f46";

/// The lit switch's ring, inside the tile (flush tiles have no room
/// outside) — an overlay with a border: Blitz draws no inset shadow.
#[component]
fn LitRing() -> Element {
    rsx! { span { style: "position: absolute; left: 0; top: 0; right: 0; bottom: 0; border: 2px solid rgba(255,255,255,0.85); box-sizing: border-box; pointer-events: none;" } }
}

/// The dock's two rows: the hold layer a third of it, the switches under
/// the feet two thirds — of the FX row's height, less the hairline between
/// and the border on top.
const HOLD_H: f64 = (super::fx_row::FX_H - 2.0) / 3.0;
const MAIN_H: f64 = (super::fx_row::FX_H - 2.0) * 2.0 / 3.0;

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
pub fn TouchSwitches(perf: PerformanceModel) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let stacks = perf.stacks.clone();
    let song = perf.songs.get(perf.song_index as usize).map(|s| s.name.clone()).unwrap_or_else(|| "—".to_string());
    let fx = !perf.fx_bypass;
    let boost = perf.boost_db.abs() > 0.01;
    // Off: the level it switches on at, as the rig keeps it.
    let boost_label = format!("{:+.0} dB", if boost { perf.boost_db } else { perf.boost_level });
    // Switch 5 a stack (the bass's fifth): its tile under the foot, and the
    // hold layer's first tile the sixth stack.
    let five_is_stack = perf.switch_actions.get(4).is_some_and(|a| a == "stack") && stacks.len() > 4;
    rsx! {
        div { style: "display: flex; flex-direction: column; background: #0a0a0c;",
            // One grid, two rows of five, the same columns: the hold layer
            // slim above, the switches under the feet tall below. Flush: a
            // hairline of the ground between.
            // As tall as the FX row it swaps with (a landscape iPhone's
            // room): the grid above never moves.
            div { style: "display: grid; grid-template-columns: repeat(5, minmax(0, 1fr)); grid-template-rows: {HOLD_H}px {MAIN_H}px; gap: 1px; background: #000; border-top: 1px solid #000; box-sizing: border-box; height: {super::fx_row::FX_H}px;",
                if let Some((i, st)) = { let i = if five_is_stack { 5 } else { 4 }; stacks.get(i).cloned().map(|s| (i, s)) } {
                    StackTile { no: 6, stack: st, index: i, compact: true }
                } else {
                    Empty { no: 6 }
                }
                FnTile { no: 7, title: "FX Toggle", subtitle: if fx { "Time FX on".to_string() } else { "Time FX off".to_string() }, bg: "#ec4899", text: WHITE, lit: fx,
                    onclick: { let rig = rig.clone(); move |_| call!(rig, |r| r.toggle_fx()) } }
                FnTile { no: 8, title: "Song", subtitle: song, bg: "#a78bfa", text: WHITE, lit: perf.perform_mode == 2,
                    onclick: { let rig = rig.clone(); move |_| call!(rig, |r| r.next_song()) } }
                FnTile { no: 9, title: "Boost", subtitle: if boost { boost_label.clone() } else { format!("{boost_label} · off") }, bg: "#fafafa", text: "#0a0a0a", lit: boost,
                    onclick: { let rig = rig.clone(); move |_| call!(rig, |r| r.toggle_boost()) } }
                // Always tuning; a press mutes the guitar into the chain to
                // tune in silence.
                TunerTile { muted: perf.tuner_visible, onclick: { let rig = rig.clone(); move |_| call!(rig, |r| r.toggle_tuner()) } }
                for i in 0..4usize {
                    if let Some(st) = stacks.get(i).cloned() {
                        StackTile { key: "m{i}", no: i as u32 + 1, stack: st, index: i, compact: false }
                    } else {
                        Empty { key: "m{i}", no: i as u32 + 1 }
                    }
                }
                if five_is_stack {
                    StackTile { no: 5, stack: stacks[4].clone(), index: 4, compact: false }
                } else {
                    TapTempo { bpm: perf.tempo_bpm }
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
fn StackTile(no: u32, stack: PerfStack, index: usize, compact: bool) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let (bg, _) = folder(&stack.name);
    let lit = stack.is_active;
    // The part a press plays: its patch, by its own name.
    let showing = stack.patches.get(stack.position as usize).cloned().unwrap_or_else(|| stack.current_patch.clone());
    let part = if showing.is_empty() { stack.name.clone() } else { showing };
    let count = stack.patches.len().max(stack.patch_count as usize);
    let pos = stack.position as usize;
    let tile_bg = if lit { bg.to_string() } else { dim(bg, 0.24) };
    // White on every stack's colour; muted while another plays.
    let ink = if lit { WHITE.to_string() } else { dim(WHITE, 0.5) };
    let dot = if compact { 5 } else { 6 };
    // A momentary switch plays while held: pressed on the way down,
    // released on the way up.
    let momentary = stack.momentary;
    let mut down = use_signal(|| false);
    let (r_click, r_down, r_up, r_leave, r_cancel) = (rig.clone(), rig.clone(), rig.clone(), rig.clone(), rig);
    rsx! {
        button {
            "aria-pressed": "{lit}",
            style: "position: relative; min-width: 0; overflow: hidden; border: none; border-radius: 0; padding: 0 8px; display: flex; flex-direction: {pick(compact, ROW, COLUMN)}; align-items: center; justify-content: center; gap: {pick(compact, 8, 4)}px; background: {tile_bg}; color: {ink}; font-family: {FONT}; cursor: pointer;",
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
            if lit { LitRing {} }
            SwitchNo { no, ink: ink.clone() }
            // The stack, small, beside the switch's number.
            span { style: "position: absolute; top: 5px; left: 24px; font-size: 12px; font-weight: 800; letter-spacing: 0.08em; text-transform: uppercase; color: {ink}; opacity: 0.85;", "{stack.name}" }
            span { style: "font-size: {name_px(&part, compact)}px; font-weight: 750; letter-spacing: 0.01em; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 100%; color: {ink};", "{part}" }
            // One grey mark at the top right when it overrides anything.
            if !stack.override_modules.is_empty() {
                span { style: "position: absolute; top: 6px; right: 8px; display: flex;",
                    super::marks::OverrideIcon { colour: INK_2.to_string(), size: if compact { 10 } else { 12 } }
                }
            }
            // The preset it loads, under its name.
            if !stack.preset.is_empty() && !compact {
                span { style: "font-size: 13px; font-weight: 600; opacity: 0.85; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 100%; color: {ink};", "{core_line(&stack.preset)}" }
            }
            if count > 1 {
                span { style: "display: flex; align-items: center; gap: {pick(compact, 4, 6)}px; margin-top: {pick(compact, 0, 4)}px;",
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
fn FnTile(no: u32, title: &'static str, subtitle: String, bg: &'static str, text: &'static str, lit: bool, onclick: EventHandler<MouseEvent>) -> Element {
    let tile_bg = if lit { bg.to_string() } else { dim(bg, 0.3) };
    let ink = if lit { text.to_string() } else { dim(text, 0.45) };
    rsx! {
        button {
            "aria-pressed": "{lit}",
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
fn Empty(no: u32) -> Element {
    rsx! {
        div { style: "position: relative; background: #0e0e11;",
            SwitchNo { no, ink: INK.to_string() }
        }
    }
}

#[component]
fn TapTempo(bpm: u32) -> Element {
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
            style: "position: relative; border: none; border-radius: 0; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 2px; background: {pick(flash(), TAP_FLASH, TAP_BG)}; color: #e4e4e7; font-family: {FONT}; cursor: pointer;",
            onclick: move |_| {
                call!(rig, |r| r.tap_tempo());
                flash.set(true);
                spawn(async move {
                    architect::platform::sleep(std::time::Duration::from_millis(90)).await;
                    flash.set(false);
                });
            },
            if beat() { LitRing {} }
            SwitchNo { no: 5, ink: INK.to_string() }
            span { style: "font-size: 20px; font-weight: 700;", "Tap Tempo" }
            span { style: "font-size: 14px; font-weight: 600; opacity: 0.8; font-variant-numeric: tabular-nums;", "{tempo} BPM" }
        }
    }
}

/// The tuner switch, always tuning: the note and how far off, the needle
/// about the middle, green when in tune. A press mutes the guitar into the
/// chain (`muted`: lit, "Mute") to tune in silence.
#[component]
fn TunerTile(muted: bool, onclick: EventHandler<MouseEvent>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut reading = use_signal(signal_guitar_proto::TunerReading::default);
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
    let (bg, ink, needle) = if in_tune { ("#14532d", "#4ade80", "#4ade80") } else { (SHEET_2, INK, "#facc15") };
    let at = 50.0 + r.cents.clamp(-50.0, 50.0);
    let note = if r.active { r.note.clone() } else { "—".to_string() };
    let cents = if r.active { format!("{:+.0}", r.cents) } else { String::new() };
    rsx! {
        button {
            style: "position: relative; min-width: 0; overflow: hidden; border: none; border-radius: 0; padding: 0 10px; display: flex; flex-direction: row; align-items: center; gap: 10px; background: {bg}; color: {ink}; font-family: {FONT}; cursor: pointer;",
            onclick: move |e| onclick.call(e),
            if muted { LitRing {} }
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
        Some(dot) => preset[dot..].find(" + ").map_or(preset, |plus| &preset[..dot + plus]),
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
