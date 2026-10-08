//! Perform mode — a 5×2 footswitch grid modeled on a 5-switch floor
//! controller with hold-layers.
//!
//! Row 1 = the physical switches (1–5): four stack folders + Tap Tempo.
//! Row 2 = each switch's HOLD function (6–10): Ambient (hold 1), FX Toggle
//! (hold 2), Song — reserved (hold 3), Boost (hold 4), Tuner (hold 5).
//! On boards with more switches, row 2 gets its own physical switches; the
//! grid is the superset either way. Purely presentational: state in via
//! [`PerformanceModel`], actions out via `Callback` props (the tuner overlay
//! reads the `RigClient` from context to poll pitch).

use std::time::Duration;

use dioxus::prelude::*;
use signal_widgets::switches::{
    HoldButton, InlineMenu, LIT_RING, LearnBadge, MENU_PANEL, MidiLearnRows, SwitchMenuFrame,
    SwitchNo, close_menu, dim, open_menu,
};

use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{PerfStack, PerformanceModel, SwitchLearn, TunerReading};


/// The jobs a footswitch can be given (the rig's `SWITCH_ACTIONS`):
/// `(key, label)`. Stepping jobs go forward on a tap, back on a hold.
const JOBS: &[(&str, &str)] = &[
    ("stack", "Its stack"),
    ("tap_tempo", "Tap tempo"),
    ("parts", "Next part · hold: previous"),
    ("sections", "Next section · hold: previous"),
    ("songs", "Next song · hold: previous"),
    ("tuner", "Tuner"),
    ("boost", "Boost"),
    ("fx", "FX toggle"),
    ("none", "Nothing"),
];

/// A job's tile title.
fn job_title(job: &str) -> &'static str {
    match job {
        "tap_tempo" => "Tap Tempo",
        "parts" => "Next Part",
        "sections" => "Next Section",
        "songs" => "Next Song",
        "tuner" => "Tuner",
        "boost" => "Boost",
        "fx" => "FX Toggle",
        "none" => "—",
        _ => "Stack",
    }
}

/// Tile background + text color for a folder (footswitch), by name. Also
/// tints the header's active-patch lens, so "where you are in the set" reads
/// at a glance from anywhere in the UI.
///
/// The usual names have their colours; any other stack (an amp's name — a
/// Dumble, a Two-Rock) takes one of a vivid set, picked by its name so it is
/// the same everywhere. Text is dark on the bright tiles: white on orange or
/// red reads at under 4:1.
pub fn folder_color(name: &str) -> (&'static str, &'static str) {
    match name.trim().to_ascii_lowercase().as_str() {
        "clean" => ("#38bdf8", "#082f49"),
        "crunch" => ("#2563eb", "#ffffff"),
        "drive" | "rhythm" => ("#f97316", "#1c0d02"),
        "lead" => ("#ef4444", "#1f0606"),
        "ambient" => ("#06b6d4", "#04222a"),
        "" | "none" | "—" => ("#3f3f46", "#e4e4e7"),
        other => {
            const SET: [(&str, &str); 8] = [
                ("#a78bfa", "#1e1b4b"), // violet
                ("#f59e0b", "#1c1002"), // amber
                ("#10b981", "#022c22"), // emerald
                ("#ec4899", "#2a0616"), // pink
                ("#6366f1", "#ffffff"), // indigo
                ("#84cc16", "#1a2e05"), // lime
                ("#14b8a6", "#042f2e"), // teal
                ("#e11d48", "#ffffff"), // rose
            ];
            // FNV-1a: stable across runs and builds.
            let h = other.bytes().fold(0x811c_9dc5_u32, |h, b| (h ^ u32::from(b)).wrapping_mul(0x0100_0193));
            SET[h as usize % SET.len()]
        }
    }
}

/// Perform-mode footswitch grid — see the module docs for the layout.
#[component]
pub fn PerformGrid(
    model: PerformanceModel,
    on_press: Callback<usize>,
    on_toggle_fx: Callback<()>,
    on_toggle_boost: Callback<()>,
    on_cycle_boost: Callback<()>,
    on_tap_tempo: Callback<()>,
    on_prev_song: Callback<()>,
    on_next_song: Callback<()>,
    on_select_song: Callback<usize>,
    /// Both rows compact and equal: the switches as a short strip, so the
    /// page above gets the height.
    #[props(default)]
    compact: bool,
) -> Element {
    // Callbacks made once per site, not once per render (see `stable`).
    let cbs = crate::stable::use_stable();
    let stacks = model.stacks;
    let rig = use_hook(try_consume_context::<RigClient>);
    let mode = model.perform_mode;
    let _set_mode = {
        let rig = rig.clone();
        move |m: u32| {
            if let Some(r) = rig.clone() {
                spawn(async move {
                    let _ = r.set_perform_mode(m).await;
                });
            }
        }
    };
    // The Song layer: switches 1/2 become Prev/Next, the middle shows the
    // setlist for fast scrolling, switch 5 exits. Entered by holding 3 or
    // tapping the Song tile.
    let mut song_layer = use_signal(|| false);

    let fx_sub = if model.fx_bypass { "Bypassed" } else { "On" };
    let boost_sub = if model.boost_db == 0.0 {
        "Off · hold: level".to_string()
    } else if model.boost_db < 0.0 {
        format!("−{} dB cut", -model.boost_db as i32)
    } else {
        format!("+{} dB", model.boost_db as i32)
    };

    // Each row-1 switch's HOLD action — the row-2 function directly below it
    // (identical to the physical footswitch): 1→Ambient, 2→FX Toggle,
    // 3→Song (reserved), 4→Boost on/off. 5→Tuner is wired on the tile.
    let hold_actions: [Option<Callback<()>>; 4] = [
        Some(cbs.cb(move |(): ()| on_press.call(4))),
        Some(cbs.cb(move |(): ()| on_toggle_fx.call(()))),
        Some(cbs.cb(move |(): ()| song_layer.set(true))),
        Some(cbs.cb(move |(): ()| on_toggle_boost.call(()))),
    ];

    // What each footswitch does right now (the song's and part's jobs).
    let jobs: Vec<String> = (0..5)
        .map(|i| {
            model.switch_actions.get(i).cloned().unwrap_or_else(|| {
                if i < 4 {
                    "stack".into()
                } else {
                    "tap_tempo".into()
                }
            })
        })
        .collect();
    let in_song = mode == 2;
    let song_changes: Vec<(String, u32)> = model
        .song_changes
        .iter()
        .map(|c| (c.patch.clone(), c.count))
        .collect();
    let song_parts: Vec<(String, String)> = if in_song {
        model
            .parts
            .iter()
            .map(|p| (p.name.clone(), p.patch.clone()))
            .collect()
    } else {
        Vec::new()
    };
    let part_name = if in_song {
        model
            .parts
            .get(model.part_index as usize)
            .map(|p| p.name.clone())
    } else {
        None
    };
    // Where a stepping switch goes on a tap (next) and a hold (back) — its
    // tile says so.
    // Past a song's last section the step goes on to the next song.
    let next_song = || {
        model
            .songs
            .get(model.song_index as usize + 1)
            .map_or("end of the set".to_string(), |s| format!("{} ›", s.name))
    };
    let step_hint = |job: &str| -> (String, String) {
        let at = model.part_index as usize;
        match job {
            "parts" => (
                model
                    .parts
                    .get(at + 1)
                    .map_or_else(next_song, |p| p.name.clone()),
                at.checked_sub(1)
                    .and_then(|i| model.parts.get(i))
                    .map_or(String::new(), |p| p.name.clone()),
            ),
            "sections" => {
                let sec = |i: usize| {
                    model
                        .parts
                        .get(i)
                        .map(|p| p.section.clone())
                        .unwrap_or_default()
                };
                let cur = sec(at);
                let next = model
                    .parts
                    .iter()
                    .skip(at + 1)
                    .find(|p| !p.section.eq_ignore_ascii_case(&cur))
                    .map_or_else(next_song, |p| p.section.clone());
                // Back: to this section's start from a later part of it, else
                // the section before.
                let first = (0..=at)
                    .rev()
                    .take_while(|&i| sec(i).eq_ignore_ascii_case(&cur))
                    .last()
                    .unwrap_or(at);
                let back = if first < at {
                    cur.clone()
                } else {
                    first.checked_sub(1).map(sec).unwrap_or_default()
                };
                (next, back)
            }
            "songs" => {
                let i = model.song_index as usize;
                (
                    model
                        .songs
                        .get(i + 1)
                        .map_or("end of the set".into(), |s| s.name.clone()),
                    i.checked_sub(1)
                        .and_then(|i| model.songs.get(i))
                        .map_or(String::new(), |s| s.name.clone()),
                )
            }
            _ => (String::new(), String::new()),
        }
    };
    // The chords: two neighbouring switches held together. Each is drawn
    // across the gap between its pair, naming where it goes.
    let chord_marks: Vec<ChordMark> = {
        let rig = rig.clone();
        let call = move |what: &'static str| {
            let rig = rig.clone();
            Callback::new(move |(): ()| {
                if let Some(r) = rig.clone() {
                    spawn(async move {
                        let _ = match what {
                            "prev" => r.prev_song().await,
                            "next" => r.next_song().await,
                            _ => r.toggle_tuner().await,
                        };
                    });
                }
            })
        };
        let i = model.song_index as usize;
        let (back, on) = if in_song {
            (
                i.checked_sub(1)
                    .and_then(|p| model.songs.get(p))
                    .map_or("start of set".to_string(), |s| s.name.clone()),
                model
                    .songs
                    .get(i + 1)
                    .map_or("end of set".to_string(), |s| s.name.clone()),
            )
        } else {
            ("Profile".to_string(), "Profile".to_string())
        };
        vec![
            ChordMark {
                gap: 0,
                label: back,
                icon: "‹",
                trailing: false,
                tint: "#a78bfa",
                onclick: (in_song).then(|| call("prev")),
            },
            ChordMark {
                gap: 2,
                label: "Tuner".to_string(),
                icon: "♪",
                trailing: false,
                tint: "#22c55e",
                onclick: Some(call("tuner")),
            },
            ChordMark {
                gap: 3,
                label: on,
                icon: "›",
                trailing: true,
                tint: "#a78bfa",
                onclick: (in_song).then(|| call("next")),
            },
        ]
    };
    let current_song = model
        .songs
        .get(model.song_index as usize)
        .map(|s| s.name.clone())
        .unwrap_or_default();
    let song_pos = format!("{}/{}", model.song_index + 1, model.songs.len().max(1));
    let desk = crate::control::use_tier() >= crate::control::Tier::Desktop;

    rsx! {
        div { class: "flex flex-col h-full min-h-0 gap-2",
        // The song and part line — on the desktop the setlist sidebar and
        // the Song switch already say it, and the room is the switches'.
        if mode == 2 && !desk {
            div { class: "flex items-center gap-2 flex-shrink-0",
                span { class: "text-xs text-muted-foreground truncate", "{current_song} · {song_pos}" }
                // The part that is up, and what it lays over the profile.
                if let Some(part) = model.parts.get(model.part_index as usize) {
                    span { class: "text-xs truncate", style: "color: #bfdbfe;",
                        if part.section.eq_ignore_ascii_case(&part.name) {
                            "Part: {part.name}"
                        } else {
                            "{part.section} › {part.name}"
                        }
                    }
                    if !part.patch.is_empty() {
                        span { class: "text-xs text-muted-foreground truncate", "→ {part.patch}" }
                    }
                    if !part.overrides.is_empty() {
                        span { class: "text-xs text-muted-foreground", "± {part.overrides.len()}" }
                    }
                }
            }
        }

        // (The pedal view was Preset mode's; there is no Preset mode now.)
    // Hold layer ABOVE the main switches (a footswitch's hold function
    // lives "up" from your toe) — a slim strip, ~1/8 the main row.
    // The switches, with the chords (two switches held together)
    // drawn across the gaps between the pair.
    div { style: "position: relative; flex: 1 1 0; min-height: 0; display: flex; flex-direction: column;",
        div {
            class: "grid grid-cols-5 gap-2 flex-1 min-h-0",
            style: if compact {
                "grid-template-rows: minmax(0, 1fr) minmax(0, 1fr);"
            } else {
                "grid-template-rows: minmax(44px, 1fr) minmax(0, 7fr);"
            },

            // ── Row A: the hold layer (switches 6–10), compact. The same in
            // every mode: song parts overlay the profile rather than taking
            // switches of their own, so Setlist mode plays the rig with the
            // same feet as Profile mode — parts are chosen from the sidebar,
            // the palette or the keymap. ──
            if let Some(stack) = stacks.get(4).cloned() {
                StackTile { index: 4usize, switch_no: 6, stack, on_press, compact: true, part: part_name.clone(), in_song, parts: song_parts.clone(), changes: song_changes.clone(), learn: model.learn.clone() }
            } else {
                div { class: "relative rounded-lg border border-dashed border-border/30",
                    SwitchNo { no: 6 }
                }
            }
            // Switch 7 (hold 2): FX Toggle — lit while the Time FX are ON.
            Learnable { target: "fx", title: "Switch 7 · FX Toggle", learn: model.learn.clone(),
                FnTile {
                    title: "FX Toggle".to_string(),
                    subtitle: fx_sub.to_string(),
                    bg: "#ec4899".to_string(),
                    text: "#ffffff".to_string(),
                    active: !model.fx_bypass,
                    switch_no: 7,
                    compact: true,
                    onclick: on_toggle_fx,
                }
            }
            // Switch 8 (hold 3): the Song layer — setlist prev/next + fast
            // scroll. Tap toggles the layer; the tile names where you are.
            FnTile {
                title: "Song".to_string(),
                subtitle: format!("{song_pos} · {current_song}"),
                bg: "#a78bfa".to_string(),
                text: "#1e1b4b".to_string(),
                active: song_layer() || model.perform_mode == 2,
                switch_no: 8,
                compact: true,
                // As the footswitch: from Profile mode, into Setlist mode on
                // the song that is up; in Setlist mode, the song layer.
                onclick: cbs.cb({
                    let rig = rig.clone();
                    let in_setlist = model.perform_mode == 2;
                    move |(): ()| {
                        if in_setlist {
                            song_layer.toggle();
                        } else if let Some(r) = rig.clone() {
                            spawn(async move {
                                let _ = r.set_perform_mode(2).await;
                            });
                        }
                    }
                }),
            }
            // Switch 9 (hold 4): Boost — tap on/off, hold rotates the level.
            Learnable { target: "boost", title: "Switch 9 · Boost", learn: model.learn.clone(),
                BoostTile {
                    subtitle: boost_sub,
                    active: model.boost_db != 0.0,
                    switch_no: 9,
                    on_toggle: on_toggle_boost,
                    on_cycle: on_cycle_boost,
                }
            }
            // Switch 10 (hold 5): the live tuner, right in the tile.
            Learnable { target: "tuner", title: "Switch 10 · Tuner", learn: model.learn.clone(),
            LiveTunerTile {
                switch_no: 10,
                onclick: cbs.cb({
                    let rig = rig.clone();
                    move |(): ()| {
                        if let Some(r) = rig.clone() {
                            spawn(async move { let _ = r.toggle_tuner().await; });
                        }
                    }
                }),
            }
            }

            // ── Row B: the Song layer (while active) or switches 1–5 ──
            if song_layer() {
                // Switch 1: previous song.
                HoldButton {
                    class: "relative flex flex-col items-center justify-center gap-1 rounded-xl h-full bg-card border border-border hover:bg-accent/40".to_string(),
                    style: String::new(),
                    on_tap: on_prev_song,
                    SwitchNo { no: 1 }
                    span { class: "text-3xl font-bold", "‹" }
                    span { class: "text-xs text-muted-foreground", "Previous" }
                }
                // Switch 2: next song.
                HoldButton {
                    class: "relative flex flex-col items-center justify-center gap-1 rounded-xl h-full bg-card border border-border hover:bg-accent/40".to_string(),
                    style: String::new(),
                    on_tap: on_next_song,
                    SwitchNo { no: 2 }
                    span { class: "text-3xl font-bold", "›" }
                    span { class: "text-xs text-muted-foreground", "Next" }
                }
                // The setlist, spanning the middle — current song highlighted,
                // any entry jumps straight there.
                div {
                    class: "relative col-span-2 rounded-xl border border-border bg-card overflow-y-auto",
                    div { class: "flex flex-col p-2 gap-1",
                        for (i, song) in model.songs.iter().enumerate() {
                            {
                                let is_current = i == model.song_index as usize;
                                let name = song.name.clone();
                                let meta = format!("{} · {}", song.key, song.bpm);
                                rsx! {
                                    button {
                                        key: "{i}",
                                        class: if is_current {
                                            "flex items-center gap-2 rounded-md px-3 py-1.5 text-left text-sm font-bold bg-accent text-accent-foreground"
                                        } else {
                                            "flex items-center gap-2 rounded-md px-3 py-1.5 text-left text-sm text-muted-foreground hover:bg-accent/40"
                                        },
                                        onclick: move |_| on_select_song.call(i),
                                        span { class: "font-mono text-[10px] opacity-60 w-4", "{i + 1}" }
                                        span { class: "truncate", "{name}" }
                                        span { class: "ml-auto font-mono text-[10px] opacity-60 flex-shrink-0", "{meta}" }
                                    }
                                }
                            }
                        }
                    }
                }
                // Switch 5: back to the rig.
                HoldButton {
                    class: "relative flex flex-col items-center justify-center gap-1 rounded-xl h-full bg-card border border-border hover:bg-accent/40".to_string(),
                    style: String::new(),
                    on_tap: cbs.cb(move |(): ()| song_layer.set(false)),
                    SwitchNo { no: 5 }
                    span { class: "text-xl font-bold", "Back" }
                    span { class: "text-xs text-muted-foreground", "to the rig" }
                }
            } else {
            for i in 0..4usize {
                if jobs[i] != "stack" {
                    ActionTile {
                        key: "a{i}",
                        footswitch: i,
                        job: jobs[i].clone(),
                        next: step_hint(&jobs[i]).0,
                        back: step_hint(&jobs[i]).1,
                        stack: stacks.get(i).cloned(),
                        part: part_name.clone(),
                        in_song,
                        compact,
                    }
                } else if let Some(stack) = stacks.get(i).cloned() {
                    StackTile {
                        key: "s{i}-{stack.is_active}",
                        index: i,
                        switch_no: i + 1,
                        stack,
                        on_press,
                        on_hold: hold_actions[i],
                        compact,
                        footswitch: Some(i),
                        part: part_name.clone(),
                        in_song,
                        parts: song_parts.clone(),
                        changes: song_changes.clone(),
                        learn: model.learn.clone(),
                    }
                } else {
                    div { key: "s{i}", class: "relative rounded-xl border-2 border-dashed border-border/30",
                        SwitchNo { no: i + 1 }
                    }
                }
            }
            // Switch 5: its job — Tap Tempo (hold: tuner) unless the song
            // gives it another (stepping through the parts, by default).
            if jobs[4] != "tap_tempo" {
                ActionTile {
                    footswitch: 4usize,
                    job: jobs[4].clone(),
                    next: step_hint(&jobs[4]).0,
                    back: step_hint(&jobs[4]).1,
                    part: part_name.clone(),
                    in_song,
                    compact,
                }
            } else {
            TapTempoTile {
                compact,
                in_song,
                part: part_name.clone(),
                tempo_bpm: model.tempo_bpm,
                // Hold is free — the tuner is switches 3 + 4 together.
                on_tap: on_tap_tempo,
                learn: model.learn.clone(),
            }
            }
            }
        }
        if !song_layer() {
            ChordMarks { chords: chord_marks.clone(), compact }
        }
        }
        }
    }
}

/// A chord: two neighbouring footswitches held together.
#[derive(Clone, PartialEq)]
pub struct ChordMark {
    /// The gap it spans: 0 = between switches 1 and 2, … 3 = between 4 and 5.
    gap: usize,
    /// Where it goes (the song or profile), or what it does.
    label: String,
    icon: &'static str,
    /// The icon after the label (forward) rather than before.
    trailing: bool,
    tint: &'static str,
    /// Clicking the mark does the chord, when it can from here.
    onclick: Option<Callback<()>>,
}

/// The chords, drawn over the bottom of the switch row: a pill on the gap
/// between the two switches, a bar tying the pair — so which feet, and what
/// it does, reads without a legend.
#[component]
fn ChordMarks(chords: Vec<ChordMark>, #[props(default)] compact: bool) -> Element {
    rsx! {
        // Zero height, so it never takes a press meant for a switch.
        div { style: format!("position: absolute; left: 0; right: 0; bottom: {}px; height: 0;", if compact { 6 } else { 14 }),
            for c in chords.iter() {
                {
                    // Gap centres: 5 equal columns with 12 px gaps.
                    // Centre of gap g: (g+1)·(W−48)/5 + 12g + 6.
                    let left = format!("calc({}% + {:.1}px)", (c.gap + 1) * 20, 2.4 * c.gap as f32 - 3.6);
                    let label = c.label.clone();
                    let onclick = c.onclick;
                    rsx! {
                        div {
                            key: "{c.gap}",
                            style: format!(
                                "position: absolute; left: {left}; bottom: 0; transform: translateX(-50%); \
                                 display: flex; align-items: center; gap: 5px; padding: 3px 8px 3px 5px; border-radius: 999px; \
                                 background: #0b0b0e; border: 1px solid {tint}66; color: #e4e4e7; white-space: nowrap; \
                                 font-size: 10px; font-weight: 600; box-shadow: 0 2px 8px #000a; cursor: {};",
                                if onclick.is_some() { "pointer" } else { "default" },
                                tint = c.tint,
                            ),
                            title: "Hold both switches together",
                            onclick: move |_| {
                                if let Some(cb) = onclick {
                                    cb.call(());
                                }
                            },
                            // Two feet: the pair's numbers, tied.
                            span { style: format!(
                                    "display: flex; align-items: center; gap: 2px; font-family: monospace; font-size: 9px; \
                                     padding: 1px 4px; border-radius: 999px; background: {}26; color: {};", c.tint, c.tint),
                                "{c.gap + 1}+{c.gap + 2}"
                            }
                            if !c.trailing {
                                span { style: "color: {c.tint}; font-size: 12px; line-height: 1;", "{c.icon}" }
                            }
                            span { style: "max-width: 120px; overflow: hidden; text-overflow: ellipsis;", "{label}" }
                            if c.trailing {
                                span { style: "color: {c.tint}; font-size: 12px; line-height: 1;", "{c.icon}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A switch's menu, by touch. On the phone it is rig edit's: nothing on the
/// switches being played (no ⋯ to catch a foot or a thumb), and in rig edit
/// the whole switch, a tap opening the menu instead of pressing it. Off the
/// phone, the ⋯ in the corner.
#[component]
fn SwitchMenuCover(onclick: EventHandler<MouseEvent>) -> Element {
    let Some(edit) = try_use_context::<crate::phone::RigEdit>() else {
        return rsx! { signal_widgets::TouchMenuButton { onclick, title: "Switch menu" } };
    };
    if !(edit.0)() {
        return rsx! {};
    }
    rsx! {
        div {
            style: "position: absolute; inset: 0; z-index: 5; box-sizing: border-box; border: 2px dashed rgba(255,255,255,0.55); \
                    border-radius: 12px; display: flex; align-items: flex-start; justify-content: flex-end; padding: 6px; cursor: pointer;",
            title: "Switch menu",
            onpointerdown: move |e: PointerEvent| e.stop_propagation(),
            onpointerup: move |e: PointerEvent| e.stop_propagation(),
            onclick: move |e: MouseEvent| {
                e.stop_propagation();
                onclick.call(e);
            },
            span {
                style: "display: flex; align-items: center; gap: 4px; padding: 3px 8px; border-radius: 999px; \
                        background: rgba(0,0,0,0.55); color: #e5e7eb; font-size: 10px; font-weight: 700; letter-spacing: 0.06em;",
                svg { width: "11", height: "11", view_box: "0 0 24 24", fill: "none", stroke: "#e5e7eb", stroke_width: "2.2",
                    stroke_linecap: "round", stroke_linejoin: "round", style: "display: block;",
                    path { d: "M4 20h4L19 9l-4-4L4 16z" }
                }
                "EDIT"
            }
        }
    }
}

/// A switch's label: one line, cut with an ellipsis if it ever does not
/// fit. Blitz kept a label broken into lines at a width it no longer had
/// (an idle page: "Two-" over "Rock", "Tap" over "Tempo" over its BPM).
const NOWRAP: &str = "white-space: nowrap; max-width: 100%; overflow: hidden; text-overflow: ellipsis;";

/// One colored footswitch folder tile. Tap = press the stack; hold (row 1)
/// = the row-2 function beneath it.
#[component]
fn StackTile(
    index: usize,
    switch_no: usize,
    stack: PerfStack,
    on_press: Callback<usize>,
    #[props(default)] on_hold: Option<Callback<()>>,
    #[props(default)] compact: bool,
    /// The footswitch (0-based) this tile is, when it is one of 1–5 — its
    /// job can then be changed from the menu.
    #[props(default)]
    footswitch: Option<usize>,
    /// The part that is up (Setlist mode).
    #[props(default)]
    part: Option<String>,
    #[props(default)] in_song: bool,
    /// The song's parts `(name, patch)` (Setlist mode), for the part menu.
    #[props(default)]
    parts: Vec<(String, String)>,
    /// The song's changes to profile patches `(patch, count)`.
    #[props(default)]
    changes: Vec<(String, u32)>,
    /// The switches' MIDI learn.
    #[props(default)]
    learn: SwitchLearn,
) -> Element {
    // Callbacks made once per site, not once per render (see `stable`).
    let cbs = crate::stable::use_stable();
    let rig = use_hook(try_consume_context::<signal_guitar_proto::rig::RigClient>);
    let learn_target = format!("stack:{index}");
    // Menus open upward in the app's popup layer (the grid is the bottom of
    // the window); in place where the app has none.
    let host = signal_widgets::PopupHost::try_use();
    let mut menu = use_signal(|| false);
    let (momentary, no_rotate) = (stack.momentary, stack.no_rotate);
    let (bg, text) = folder_color(&stack.name);
    // Lit or dark as colours, not as an opacity class: the renderer could
    // keep a tile's old opacity when only its class changed (every switch
    // that had been active stayed lit, the ring alone moving), and a
    // footswitch's state is the one thing that must never be stale.
    let (bg, text, state_style) = if stack.is_active {
        (bg.to_string(), text.to_string(), LIT_RING)
    } else {
        (dim(bg, 0.24), dim(text, 0.35), "")
    };
    let state_cls = "";
    let layout_cls = if compact {
        "relative flex items-center justify-center gap-2 rounded-lg"
    } else {
        "relative flex flex-col items-center justify-center gap-1 rounded-xl"
    };
    let open_switch_menu = EventHandler::new({
        let (stack, part, parts, changes, learn, target) =
            (stack.clone(), part.clone(), parts.clone(), changes.clone(), learn.clone(), learn_target.clone());
        move |e: MouseEvent| {
            e.prevent_default();
            let Some(h) = host else {
                menu.set(true);
                return;
            };
            let (stack, part, parts, changes, learn, target) =
                (stack.clone(), part.clone(), parts.clone(), changes.clone(), learn.clone(), target.clone());
            open_menu(h, &e, move || rsx! {
                SwitchMenu {
                    switch_no,
                    footswitch,
                    stack: Some((index, stack.clone())),
                    job: "stack".to_string(),
                    part: part.clone(),
                    in_song,
                    parts: parts.clone(),
                    changes: changes.clone(),
                    learn: learn.clone(),
                    learn_target: Some(target.clone()),
                    on_close: move |()| close_menu(Some(h)),
                }
            });
        }
    });
    rsx! {
        div {
            // Its own text stays inside it: a long sound line used to run
            // across the neighbouring switches.
            style: "position: relative; height: 100%; display: flex; flex-direction: column; min-width: 0; overflow: hidden;",
            // Right-click: how this switch behaves, for the song that is up.
            oncontextmenu: move |e: MouseEvent| open_switch_menu.call(e),
            onmouseleave: move |_| menu.set(false),
            LearnBadge {
                binding: learn.binding(&learn_target).map(str::to_string),
                learning: learn.is_learning(&learn_target),
            }
        HoldButton {
            class: format!("{layout_cls} h-full {state_cls}"),
            style: format!("background-color: {bg}; color: {text}; min-width: 0; overflow: hidden; padding: 0 8px; {state_style}"),
            on_tap: cbs.cb(move |(): ()| on_press.call(index)),
            on_hold,
            on_down: momentary.then(|| cbs.cb(move |(): ()| on_press.call(index))),
            on_up: momentary.then(|| {
                let rig = rig.clone();
                cbs.cb(move |(): ()| {
                    if let Some(r) = rig.clone() {
                        spawn(async move {
                            let _ = r.release_stack(index as u32).await;
                        });
                    }
                })
            }),
            SwitchNo { no: switch_no }
            // How the switch behaves, when it is not the usual latch-and-rotate
            // — and whether the part that is up tunes it.
            if momentary || no_rotate || stack.part_tuned {
                span {
                    style: "position: absolute; top: 6px; left: 50%; transform: translateX(-50%); white-space: nowrap; \
                            font-size: 8px; font-weight: 800; letter-spacing: 0.12em; opacity: 0.85;",
                    {
                        let mut tags = Vec::new();
                        if stack.part_tuned { tags.push("PART"); }
                        if momentary { tags.push("HOLD"); }
                        if no_rotate { tags.push("NO ROTATE"); }
                        tags.join(" · ")
                    }
                }
            }
            // Amber dot while the current patch is still loading.
            if !stack.available {
                span { class: "absolute top-2 right-2 w-2.5 h-2.5 rounded-full",
                    style: "background-color: #fde047;" }
            }
            span {
                class: if compact { "text-sm font-bold tracking-wide" } else { "text-2xl font-bold tracking-wide" },
                style: NOWRAP,
                "{stack.name}"
            }
            // Folder-as-main: the stack name IS the main sound — only
            // variations get a sub-label.
            if stack.current_patch != "Default" && !stack.current_patch.eq_ignore_ascii_case(&stack.name) {
                span {
                    class: if compact { "text-[10px] font-semibold opacity-80" } else { "text-sm font-semibold opacity-90" },
                    style: NOWRAP,
                    "{stack.current_patch}"
                }
            }
            // The preset this patch points at + which modules it overrides.
            if !compact {
                // The sound, a line per part ("Deluxe + AC30" over "Clean +
                // Ambience"): lines that never wrap size to themselves, so the
                // switch's button centres them, and the tile clips what is
                // left. (Wrapped text in the button collapsed a word wide.)
                for (n, line) in stack.preset.split(" · ").enumerate() {
                    div {
                        key: "{n}",
                        style: "max-width: 100%; white-space: nowrap; overflow: hidden; font-size: 11px; \
                                line-height: 1.25; opacity: 0.7;",
                        "{line}"
                    }
                }
                if !stack.override_modules.is_empty() {
                    div { style: "display: flex; justify-content: center; gap: 6px;",
                        for m in stack.override_modules.iter() {
                            span {
                                key: "{m}",
                                class: "opacity-80",
                                title: "overrides {m}",
                                crate::icons::ModuleGlyph { module: m.clone(), size: 11 }
                            }
                        }
                    }
                }
            } else if !stack.override_modules.is_empty() {
                span { class: "opacity-70", style: "display: flex; gap: 3px;",
                    for m in stack.override_modules.iter() {
                        crate::icons::ModuleGlyph { key: "{m}", module: m.clone(), size: 10 }
                    }
                }
            }
            // Rotation dots — one per patch in the folder, current one lit.
            // Reads as "where the next press lands" without any counting.
            if stack.patch_count > 1 {
                div { class: if compact { "flex items-center gap-1" } else { "flex items-center gap-1.5 mt-1" },
                    for i in 0..stack.patch_count {
                        span {
                            key: "{i}",
                            class: if i == stack.position {
                                "w-1.5 h-1.5 rounded-full bg-current opacity-95"
                            } else {
                                "w-1.5 h-1.5 rounded-full bg-current opacity-30"
                            },
                        }
                    }
                }
            }
        }
            if menu() {
                InlineMenu {
                SwitchMenu {
                    switch_no,
                    footswitch,
                    stack: Some((index, stack.clone())),
                    job: "stack".to_string(),
                    part: part.clone(),
                    in_song,
                    parts: parts.clone(),
                    changes: changes.clone(),
                    learn: learn.clone(),
                    learn_target: Some(learn_target.clone()),
                    on_close: move |()| menu.set(false),
                }
                }
            }
            SwitchMenuCover { onclick: move |e: MouseEvent| open_switch_menu.call(e) }
        }
    }
}

/// A footswitch given a job other than its usual one (Next Part, Tuner…):
/// tap does it, hold does its back (stepping) or the hold layer. Right-click
/// to change it.
#[component]
fn ActionTile(
    footswitch: usize,
    job: String,
    /// Where a stepping job goes on a tap, and on a hold.
    #[props(default)]
    next: String,
    #[props(default)] back: String,
    /// The stack it would play, for turning it back into one from the menu.
    #[props(default)]
    stack: Option<PerfStack>,
    #[props(default)] part: Option<String>,
    #[props(default)] in_song: bool,
    #[props(default)] compact: bool,
) -> Element {
    let cbs = crate::stable::use_stable();
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut menu = use_signal(|| false);
    let host = signal_widgets::PopupHost::try_use();
    let sw = footswitch as u32;
    let open_switch_menu = EventHandler::new({
        let (job, stack, part) = (job.clone(), stack.clone(), part.clone());
        move |e: MouseEvent| {
            e.prevent_default();
            let Some(h) = host else {
                menu.set(true);
                return;
            };
            let (job, stack, part) = (job.clone(), stack.clone(), part.clone());
            open_menu(h, &e, move || rsx! {
                SwitchMenu {
                    switch_no: footswitch + 1,
                    footswitch: Some(footswitch),
                    stack: stack.clone().map(|st| (footswitch, st)),
                    job: job.clone(),
                    part: part.clone(),
                    in_song,
                    on_close: move |()| close_menu(Some(h)),
                }
            });
        }
    });
    rsx! {
        div {
            style: "position: relative; height: 100%; display: flex; flex-direction: column;",
            oncontextmenu: move |e: MouseEvent| open_switch_menu.call(e),
            onmouseleave: move |_| menu.set(false),
            HoldButton {
                class: if compact {
                    "relative flex items-center justify-center gap-2 rounded-lg h-full opacity-90 hover:opacity-100".to_string()
                } else {
                    "relative flex flex-col items-center justify-center gap-1 rounded-xl h-full opacity-90 hover:opacity-100".to_string()
                },
                style: "background-color: #1e1b4b; color: #c7d2fe; box-shadow: inset 0 0 0 1px #4338ca;".to_string(),
                on_tap: cbs.cb({
                    let rig = rig.clone();
                    move |(): ()| {
                        if let Some(r) = rig.clone() {
                            spawn(async move { let _ = r.tap_switch(sw).await; });
                        }
                    }
                }),
                on_hold: Some(cbs.cb({
                    let rig = rig.clone();
                    move |(): ()| {
                        if let Some(r) = rig.clone() {
                            spawn(async move { let _ = r.hold_switch(sw).await; });
                        }
                    }
                })),
                SwitchNo { no: footswitch + 1 }
                if matches!(job.as_str(), "parts" | "sections" | "songs") {
                    // A stepping switch leads with where it goes.
                    span { class: "text-[9px] font-bold tracking-[0.14em] uppercase opacity-60", "{job_title(&job)}" }
                    span {
                        class: if compact { "text-sm font-bold truncate max-w-full px-2" } else { "text-2xl font-bold truncate max-w-full px-2" },
                        style: "color: #e0e7ff;",
                        "{next}"
                    }
                    if !back.is_empty() {
                        span { class: if compact { "text-[9px] opacity-60 truncate max-w-full" } else { "text-[11px] opacity-60 truncate max-w-full" },
                        style: NOWRAP,
                            "hold: ‹ {back}"
                        }
                    }
                } else {
                    span {
                        class: if compact { "text-sm font-bold tracking-wide" } else { "text-xl font-bold tracking-wide" },
                        style: NOWRAP,
                        "{job_title(&job)}"
                    }
                }
            }
            if menu() {
                InlineMenu {
                    SwitchMenu {
                        switch_no: footswitch + 1,
                        footswitch: Some(footswitch),
                        stack: stack.clone().map(|st| (footswitch, st)),
                        job: job.clone(),
                        part: part.clone(),
                        in_song,
                        on_close: move |()| menu.set(false),
                    }
                }
            }
            SwitchMenuCover { onclick: move |e: MouseEvent| open_switch_menu.call(e) }
        }
    }
}

/// A switch's right-click menu: for the whole song or just the part that
/// is up — how its stack behaves (momentary, no stacking), which patches it
/// rotates through, and what job the footswitch does.
#[component]
fn SwitchMenu(
    switch_no: usize,
    /// The footswitch (0-based), when the tile is one of 1–5.
    footswitch: Option<usize>,
    /// The stack (index, state), for a stack switch.
    stack: Option<(usize, PerfStack)>,
    job: String,
    part: Option<String>,
    in_song: bool,
    on_close: Callback<()>,
    /// The song's parts `(name, patch)`, for the switch's part menu.
    #[props(default)]
    parts: Vec<(String, String)>,
    /// The song's changes to profile patches `(patch, count)`.
    #[props(default)]
    changes: Vec<(String, u32)>,
    /// The switches' MIDI learn, and this switch's name in it — the menu
    /// ends with its MIDI learn rows.
    #[props(default)]
    learn: SwitchLearn,
    #[props(default)]
    learn_target: Option<String>,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let host = signal_widgets::PopupHost::try_use();
    // Per part when a part is up — the reason to open this in a song.
    let mut for_part = use_signal(|| part.is_some());
    let mut show_patches = use_signal(|| false);
    let patches = use_resource({
        let rig = rig.clone();
        move || {
            let rig = rig.clone();
            async move {
                match rig {
                    Some(r) => r.patches().await.unwrap_or_default(),
                    None => Vec::new(),
                }
            }
        }
    });
    let scope_part = for_part() && part.is_some();
    let scope = match (&part, scope_part, in_song) {
        (Some(p), true, _) => format!("part · {p}"),
        (_, _, true) => "this song".to_string(),
        _ => "the profile".to_string(),
    };
    let row = "display: flex; align-items: center; gap: 8px; padding: 6px 9px; border-radius: 6px; \
               font-size: 11px; color: #d4d4d8; cursor: pointer; white-space: nowrap;";
    let head = "padding: 6px 9px 3px; font-size: 9px; letter-spacing: 0.12em; text-transform: uppercase; color: #71717a;";
    let tune = {
        let rig = rig.clone();
        move |index: usize, patches: Vec<String>, momentary: bool, no_rotate: bool| {
            if let Some(r) = rig.clone() {
                let t = signal_guitar_proto::SwitchTuning {
                    index: index as u32,
                    patches,
                    momentary,
                    no_rotate,
                    part: scope_part,
                };
                spawn(async move {
                    let _ = r.tune_switch(t).await;
                });
            }
        }
    };
    let set_job = {
        let rig = rig.clone();
        move |action: String| {
            if let (Some(r), Some(sw)) = (rig.clone(), footswitch) {
                spawn(async move {
                    let _ = r.set_switch_action(sw as u32, action, scope_part).await;
                });
            }
        }
    };
    let reset = {
        let rig = rig.clone();
        let index = stack.as_ref().map(|(i, _)| *i);
        let footswitch = footswitch;
        move || {
            if let Some(r) = rig.clone() {
                spawn(async move {
                    if let Some(i) = index {
                        let _ = r.reset_switch(i as u32, scope_part).await;
                    }
                    if let Some(sw) = footswitch {
                        let _ = r
                            .set_switch_action(sw as u32, String::new(), scope_part)
                            .await;
                    }
                });
            }
        }
    };
    let patch_list = patches.read().clone().unwrap_or_default();
    rsx! {
        div {
            style: "{MENU_PANEL}",
            div { style: "{head}", "Switch {switch_no} · {scope}" }
            // The patch this switch is on, as a part of the song: make one
            // of it, or go to / rename / remove the one it is.
            if let Some((_, st)) = stack.clone().filter(|_| in_song) {
                {
                    let patch = crate::part_menu::stack_patch(&st);
                    let is_part = parts.iter().any(|(_, p)| !patch.is_empty() && p.eq_ignore_ascii_case(&patch));
                    let changed = changes.iter().any(|(p, _)| p.eq_ignore_ascii_case(&patch));
                    let label = match (is_part, changed) {
                        (_, true) => "Part & song changes…".to_string(),
                        (true, false) => "Part…".to_string(),
                        (false, false) => format!("Make a part from {patch}…"),
                    };
                    let parts = parts.clone();
                    let changes = changes.clone();
                    let rig = rig.clone();
                    rsx! {
                        if !patch.is_empty() {
                            div {
                                class: "hover:bg-accent/40",
                                style: "{row}",
                                onclick: move |e: MouseEvent| {
                                    let items = crate::part_menu::items_with_changes(&parts, &changes, &patch);
                                    let (rig, parts, patch) = (rig.clone(), parts.clone(), patch.clone());
                                    crate::kit::context_menu(host, &e, items, EventHandler::new(move |p: crate::kit::Picked| {
                                        crate::part_menu::act(&rig, &parts, &patch, p);
                                    }));
                                    on_close.call(());
                                },
                                span { style: "width: 12px; text-align: center; color: #a78bfa;", "◆" }
                                "{label}"
                            }
                        }
                    }
                }
            }
            // Whole song ↔ the part that is up.
            if let Some(p) = part.clone() {
                div { style: "display: flex; gap: 4px; padding: 2px 6px 6px;",
                    for (label, want) in [("Whole song".to_string(), false), (format!("Part · {p}"), true)] {
                        button {
                            key: "{want}",
                            style: format!(
                                "flex: 1; padding: 4px 6px; border-radius: 6px; font-size: 10px; border: 1px solid {}; \
                                 background: {}; color: {}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                                if for_part() == want { "#6366f1" } else { "#2b2b31" },
                                if for_part() == want { "#1e1b4b" } else { "transparent" },
                                if for_part() == want { "#e0e7ff" } else { "#a1a1aa" },
                            ),
                            onclick: move |_| for_part.set(want),
                            "{label}"
                        }
                    }
                }
            }
            if let Some((index, st)) = stack.clone().filter(|_| job == "stack") {
                for (label, on, flip) in [("Momentary — only while held", st.momentary, 0u8), ("Disable stacking", st.no_rotate, 1u8)] {
                    {
                        let tune = tune.clone();
                        let (m, n) = (st.momentary, st.no_rotate);
                        rsx! {
                            div {
                                key: "{flip}",
                                class: "hover:bg-accent/40",
                                style: "{row}",
                                onclick: move |_| {
                                    on_close.call(());
                                    if flip == 0 { tune(index, Vec::new(), !m, n) } else { tune(index, Vec::new(), m, !n) }
                                },
                                span { style: "width: 12px; text-align: center; color: #22c55e;", if on { "✓" } else { "" } }
                                "{label}"
                            }
                        }
                    }
                }
                div {
                    class: "hover:bg-accent/40",
                    style: "{row}",
                    onclick: move |_| show_patches.toggle(),
                    span { style: "width: 12px; text-align: center;", if show_patches() { "▾" } else { "▸" } }
                    "Patches · {st.patches.len()} in rotation"
                }
                if show_patches() {
                    // Click to add a patch to the rotation (numbered in order)
                    // or take it out.
                    for p in patch_list.iter() {
                        {
                            let at = st.patches.iter().position(|x| x.eq_ignore_ascii_case(&p.name));
                            let name = p.name.clone();
                            let group = p.stack.clone();
                            let current = st.patches.clone();
                            let tune = tune.clone();
                            let (m, n) = (st.momentary, st.no_rotate);
                            rsx! {
                                div {
                                    key: "{name}",
                                    class: "hover:bg-accent/40",
                                    style: "{row} padding-left: 18px;",
                                    onclick: move |_| {
                                        let mut next = current.clone();
                                        match at {
                                            Some(i) => { next.remove(i); }
                                            None => next.push(name.clone()),
                                        }
                                        if !next.is_empty() {
                                            tune(index, next, m, n);
                                        }
                                    },
                                    span { style: "width: 14px; text-align: center; color: #22c55e; font-family: monospace;",
                                        if let Some(i) = at { "{i + 1}" } else { "" }
                                    }
                                    span { "{p.name}" }
                                    if !group.is_empty() {
                                        span { style: "margin-left: auto; font-size: 9px; color: #52525b;", "{group}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if footswitch.is_some() && in_song {
                div { style: "{head}", "Job" }
                for (key, label) in JOBS.iter() {
                    {
                        let set_job = set_job.clone();
                        let on = job == *key;
                        let key = key.to_string();
                        rsx! {
                            div {
                                key: "{key}",
                                class: "hover:bg-accent/40",
                                style: "{row}",
                                onclick: move |_| {
                                    on_close.call(());
                                    set_job(key.clone());
                                },
                                span { style: "width: 12px; text-align: center; color: #22c55e;", if on { "✓" } else { "" } }
                                "{label}"
                            }
                        }
                    }
                }
            }
            if in_song {
                div {
                    class: "hover:bg-accent/40",
                    style: "{row} color: #fca5a5;",
                    onclick: move |_| {
                        on_close.call(());
                        reset();
                    },
                    span { style: "width: 12px;" }
                    if scope_part { "Reset for this part" } else { "Reset for this song" }
                }
            }
            if let Some(target) = learn_target {
                LearnRows { target, learn, on_close }
            }
        }
    }
}

/// A switch menu's MIDI learn rows, on the rig's client.
#[component]
fn LearnRows(target: String, learn: SwitchLearn, on_close: Callback<()>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let call = move |f: fn(RigClient, String) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()>>>| {
        let rig = rig.clone();
        move |t: String| {
            if let Some(r) = rig.clone() {
                spawn(f(r, t));
            }
        }
    };
    let learn_it = call(|r, t| Box::pin(async move { let _ = r.midi_learn(t).await; }));
    let cancel = call(|r, _| Box::pin(async move { let _ = r.midi_learn_cancel().await; }));
    let unlearn = call(|r, t| Box::pin(async move { let _ = r.midi_unlearn(t).await; }));
    rsx! {
        MidiLearnRows {
            target: target.clone(),
            binding: learn.binding(&target).map(str::to_string),
            learning: learn.is_learning(&target),
            on_learn: move |t: String| learn_it(t),
            on_cancel: move |()| cancel(String::new()),
            on_unlearn: move |t: String| unlearn(t),
            on_close: move |()| on_close.call(()),
        }
    }
}

/// A switch with no menu of its own (FX, Boost, the tuner), made
/// learnable: right-click for its MIDI learn rows; its badge in the corner.
#[component]
fn Learnable(target: String, title: String, learn: SwitchLearn, children: Element) -> Element {
    let mut menu = use_signal(|| false);
    let host = signal_widgets::PopupHost::try_use();
    let open_switch_menu = EventHandler::new({
        let (target, title, learn) = (target.clone(), title.clone(), learn.clone());
        move |e: MouseEvent| {
            e.prevent_default();
            let Some(h) = host else {
                menu.set(true);
                return;
            };
            let (target, title, learn) = (target.clone(), title.clone(), learn.clone());
            open_menu(h, &e, move || rsx! {
                SwitchMenuFrame { title: title.clone(),
                    LearnRows { target: target.clone(), learn: learn.clone(), on_close: move |()| close_menu(Some(h)) }
                }
            });
        }
    });
    rsx! {
        div {
            style: "position: relative; height: 100%; min-height: 0; display: flex; flex-direction: column;",
            oncontextmenu: move |e: MouseEvent| open_switch_menu.call(e),
            onmouseleave: move |_| menu.set(false),
            LearnBadge {
                binding: learn.binding(&target).map(str::to_string),
                learning: learn.is_learning(&target),
            }
            {children}
            if menu() {
                InlineMenu {
                    SwitchMenuFrame { title: title.clone(),
                        LearnRows { target: target.clone(), learn: learn.clone(), on_close: move |()| menu.set(false) }
                    }
                }
            }
            SwitchMenuCover { onclick: move |e: MouseEvent| open_switch_menu.call(e) }
        }
    }
}

/// A function-switch tile (FX Toggle, Tuner).
#[component]
fn FnTile(
    title: String,
    subtitle: String,
    bg: String,
    text: String,
    active: bool,
    switch_no: usize,
    #[props(default)] compact: bool,
    onclick: Callback<()>,
) -> Element {
    // Colours, not an opacity class (see `StackTile`).
    let (bg, text, ring) = if active {
        (bg.clone(), text.clone(), LIT_RING)
    } else {
        (dim(&bg, 0.3), dim(&text, 0.45), "")
    };
    let layout_cls = if compact {
        "relative flex items-center justify-center gap-2 rounded-lg"
    } else {
        "relative flex flex-col items-center justify-center gap-1 rounded-xl"
    };
    rsx! {
        button {
            key: "{active}",
            class: format!("{layout_cls} h-full"),
            style: "background-color: {bg}; color: {text}; {ring}",
            onclick: move |_| onclick.call(()),
            SwitchNo { no: switch_no }
            span {
                class: if compact { "text-sm font-bold tracking-wide" } else { "text-xl font-bold tracking-wide" },
                style: NOWRAP,
                "{title}"
            }
            if !subtitle.is_empty() {
                span {
                    class: if compact { "text-[10px] opacity-80" } else { "text-xs opacity-80" },
                    style: NOWRAP,
                    "{subtitle}"
                }
            }
        }
    }
}

/// The boost pedal tile: tap = on/off at the remembered level, hold =
/// rotate the level (+1 → +2 → +3 → −1 dB). Mirrors the physical
/// footswitch's tap/hold split.
#[component]
fn BoostTile(
    subtitle: String,
    active: bool,
    switch_no: usize,
    on_toggle: Callback<()>,
    on_cycle: Callback<()>,
) -> Element {
    // Colours, not an opacity class (see `StackTile`).
    let style = if active {
        format!("background-color: #fafafa; color: #0a0a0a; {LIT_RING}")
    } else {
        format!(
            "background-color: {}; color: {};",
            dim("#fafafa", 0.3),
            dim("#0a0a0a", 0.45)
        )
    };
    rsx! {
        HoldButton {
            key: "{active}",
            class: "relative flex items-center justify-center gap-2 rounded-lg h-full".to_string(),
            style,
            on_tap: on_toggle,
            on_hold: Some(on_cycle),
            SwitchNo { no: switch_no }
            span { class: "text-sm font-bold tracking-wide", style: NOWRAP, "Boost" }
            span { class: "text-[10px] opacity-80", style: NOWRAP, "{subtitle}" }
        }
    }
}

/// Tap Tempo tile — muted like the other function tiles, with a ring around
/// the block flashing at the current tempo (the tile *is* the metronome).
/// Tap = tempo tap. Its hold is free (the tuner is switches 3 + 4).
#[component]
fn TapTempoTile(
    tempo_bpm: u32,
    on_tap: Callback<()>,
    #[props(default)] compact: bool,
    #[props(default)] in_song: bool,
    #[props(default)] part: Option<String>,
    /// The switches' MIDI learn.
    #[props(default)]
    learn: SwitchLearn,
) -> Element {
    let mut lit = use_signal(|| false);
    let mut menu = use_signal(|| false);
    let host = signal_widgets::PopupHost::try_use();

    // Props aren't reactive — mirror the tempo into a signal so the blink
    // loop restarts the moment a tap changes it.
    let mut bpm_sig = use_signal(|| tempo_bpm);
    if bpm_sig() != tempo_bpm {
        bpm_sig.set(tempo_bpm);
    }
    // Flash the ring on the beat: on for the front edge, off for the rest.
    // `use_resource` re-runs (dropping the old loop) when `bpm_sig` changes.
    let _blink = use_resource(move || async move {
        let bpm = u64::from(bpm_sig().max(40));
        loop {
            let beat_ms = 60_000 / bpm;
            lit.set(true);
            architect::platform::sleep(Duration::from_millis((beat_ms / 4).max(60))).await;
            lit.set(false);
            architect::platform::sleep(Duration::from_millis((beat_ms * 3 / 4).max(60))).await;
        }
    });
    let ring = if lit() {
        "box-shadow: 0 0 0 3px #fafafa, 0 0 18px #fafafa50;"
    } else {
        "box-shadow: 0 0 0 3px transparent;"
    };
    let open_switch_menu = EventHandler::new({
        let (part, learn) = (part.clone(), learn.clone());
        move |e: MouseEvent| {
            e.prevent_default();
            let Some(h) = host else {
                menu.set(true);
                return;
            };
            let (part, learn) = (part.clone(), learn.clone());
            open_menu(h, &e, move || rsx! {
                SwitchMenu {
                    switch_no: 5usize,
                    footswitch: Some(4usize),
                    stack: None,
                    job: "tap_tempo".to_string(),
                    part: part.clone(),
                    in_song,
                    learn: learn.clone(),
                    learn_target: Some("tap".to_string()),
                    on_close: move |()| close_menu(Some(h)),
                }
            });
        }
    });
    rsx! {
        div {
            style: "position: relative; height: 100%; display: flex; flex-direction: column;",
            // Right-click: MIDI-learn a pedal onto it, and (in a song) give
            // switch 5 another job.
            oncontextmenu: move |e: MouseEvent| open_switch_menu.call(e),
            onmouseleave: move |_| menu.set(false),
            LearnBadge {
                binding: learn.binding("tap").map(str::to_string),
                learning: learn.is_learning("tap"),
            }
        HoldButton {
            class: "relative flex flex-col items-center justify-center gap-1 rounded-xl h-full opacity-90 hover:opacity-100".to_string(),
            style: format!("background-color: #27272a; color: #d4d4d8; {ring}"),
            on_tap,
            SwitchNo { no: 5 }
            span {
                class: if compact { "text-sm font-bold tracking-wide" } else { "text-lg font-bold tracking-wide" },
                style: NOWRAP,
                "Tap Tempo"
            }
            span {
                class: if compact { "text-[10px] text-zinc-500" } else { "text-[11px] text-zinc-500" },
                style: NOWRAP,
                "{tempo_bpm} BPM"
            }
        }
            if menu() {
                InlineMenu {
                    SwitchMenu {
                        switch_no: 5usize,
                        footswitch: Some(4usize),
                        stack: None,
                        job: "tap_tempo".to_string(),
                        part: part.clone(),
                        in_song,
                        learn: learn.clone(),
                        learn_target: Some("tap".to_string()),
                        on_close: move |()| menu.set(false),
                    }
                }
            }
            SwitchMenuCover { onclick: move |e: MouseEvent| open_switch_menu.call(e) }
        }
    }
}

/// The hold-layer tuner tile, live: polls the tuner while the grid is
/// mounted and shows note + needle right in the slot; tap for fullscreen.
#[component]
fn LiveTunerTile(switch_no: usize, onclick: Callback<()>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut reading = use_signal(TunerReading::default);
    {
        let rig = rig;
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
    }
    let r = reading();
    let in_tune = r.active && r.cents.abs() <= 5.0;
    let needle = 50.0 + r.cents.clamp(-50.0, 50.0);
    rsx! {
        button {
            class: "relative flex flex-col justify-center gap-1 rounded-lg h-full px-2 pt-3 pb-1.5 text-left min-h-0 overflow-hidden",
            style: if in_tune {
                "background-color: #14532d; border: 1px solid #22c55e;"
            } else {
                "background-color: #1c2e26; border: 1px solid #2a4438;"
            },
            onclick: move |_| onclick.call(()),
            span { class: "absolute top-0.5 left-1.5 text-[10px] font-mono opacity-40", "{switch_no}" }
            span { class: "absolute top-0.5 right-1.5 text-[8px] font-mono opacity-40", title: "Hold switches 3 and 4 together", "3+4" }
            // Note and cents on one line, the needle under them — nothing
            // shares a corner with the switch number or the 3+4 hint.
            div { class: "flex items-baseline justify-center gap-2 min-w-0",
                span {
                    class: "text-sm font-bold leading-none",
                    style: if in_tune { "color: #22c55e; white-space: nowrap;" } else if r.active { "color: #e4e4e7; white-space: nowrap;" } else { "color: #6b7280; white-space: nowrap;" },
                    if r.active { "{r.note}" } else { "Tuner" }
                }
                if r.active {
                    span { class: "text-[10px] font-mono opacity-70", style: NOWRAP, {format!("{:+.0}", r.cents)} }
                }
            }
            div { class: "relative h-2 w-full min-w-0",
                div { class: "absolute inset-x-0 top-1/2 h-px bg-white/20" }
                div { class: "absolute left-1/2 top-0 bottom-0 w-px bg-white/40" }
                if r.active {
                    div {
                        class: "absolute top-0 bottom-0 w-0.5 rounded",
                        style: if in_tune { "left: {needle}%; background-color: #22c55e;" } else { "left: {needle}%; background-color: #eab308;" },
                    }
                }
            }
        }
    }
}

/// Full-screen tuner. Polls the rig's `tuner()` reading (~10 Hz) while
/// open; click anywhere (or the footswitch again) to close.
#[component]
pub fn TunerOverlay(on_close: EventHandler<()>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut reading = use_signal(TunerReading::default);
    // EMA-smoothed cents so the needle glides instead of jittering.
    let mut smooth_cents = use_signal(|| 0.0f32);

    {
        let rig = rig;
        use_future(move || {
            let rig = rig.clone();
            async move {
                let Some(rig) = rig else { return };
                loop {
                    if let Ok(r) = rig.tuner().await {
                        if r.active {
                            let prev = *smooth_cents.peek();
                            smooth_cents.set((r.cents - prev).mul_add(0.35, prev));
                        }
                        reading.set(r);
                    }
                    architect::platform::sleep(Duration::from_millis(60)).await;
                }
            }
        });
    }

    let r = reading();
    let cents = if r.active { smooth_cents() } else { 0.0 };
    // Needle position: −50..+50 cents → 0..100%.
    let needle_pct = 50.0 + cents.clamp(-50.0, 50.0);
    let in_tune = r.active && cents.abs() <= 5.0;
    let needle_color = if in_tune { "#22c55e" } else { "#eab308" };

    rsx! {
        div {
            class: "fixed inset-0 z-50 flex flex-col items-center justify-center gap-6 bg-black/90",
            onclick: move |_| on_close.call(()),

            span { class: "text-[10px] font-semibold uppercase tracking-[3px] text-muted-foreground",
                "Tuner"
            }
            // The note, huge — the only thing that matters from stage distance.
            span {
                class: "text-[96px] font-bold leading-none",
                style: if in_tune { "color: #22c55e;" } else if r.active { "color: #e4e4e7;" } else { "color: #3f3f46;" },
                if r.active { "{r.note}" } else { "—" }
            }
            // Frequency + cents detail under the note.
            div { class: "flex items-baseline gap-4 font-mono",
                span { class: "text-xl text-muted-foreground",
                    if r.active { {format!("{:.1} Hz", r.freq_hz)} } else { "—" }
                }
                span {
                    class: "text-xl",
                    style: if in_tune { "color: #22c55e;" } else { "color: #eab308;" },
                    if r.active { {format!("{cents:+.1} ¢")} } else { "" }
                }
            }

            // Cents scale: center line = in tune, needle shows offset.
            div { class: "relative w-[420px] max-w-[80vw] h-10",
                // Scale line + center mark.
                div { class: "absolute inset-x-0 top-1/2 h-px bg-zinc-700" }
                div { class: "absolute left-1/2 top-1 bottom-1 w-px bg-zinc-500" }
                // Needle.
                if r.active {
                    div {
                        class: "absolute top-0 bottom-0 w-1 rounded",
                        style: "left: {needle_pct}%; background-color: {needle_color};",
                    }
                }
            }
            span { class: "text-sm font-mono text-muted-foreground",
                if r.active {
                    {format!("{:+.0} cents · {:.1} Hz", r.cents, r.freq_hz)}
                } else {
                    "listening…"
                }
            }
            span { class: "text-xs text-muted-foreground/60", "tap anywhere to close" }
        }
    }
}
