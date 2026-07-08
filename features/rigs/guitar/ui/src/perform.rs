//! Perform-mode footswitch grid — five colored folder tiles plus Tap Tempo,
//! FX Toggle, and Volume Boost function switches. Purely presentational:
//! state in via [`PerformanceModel`], actions out via `Callback` props.

use std::time::Duration;

use dioxus::prelude::*;

use signal_guitar_proto::{PerfStack, PerformanceModel};

/// Tile background + text color for a folder (footswitch), by name. Also
/// tints the header's active-patch lens, so "where you are in the set" reads
/// at a glance from anywhere in the UI.
pub fn folder_color(name: &str) -> (&'static str, &'static str) {
    match name.to_ascii_lowercase().as_str() {
        "clean" => ("#38bdf8", "#082f49"),   // light blue / dark text
        "crunch" => ("#2563eb", "#ffffff"),  // darker blue / white
        "drive" => ("#f97316", "#ffffff"),   // orange / white
        "lead" => ("#ef4444", "#ffffff"),    // red / white
        "ambient" => ("#06b6d4", "#04222a"), // cyan / dark text
        _ => ("#3f3f46", "#e4e4e7"),         // zinc fallback
    }
}

/// Perform-mode footswitch grid: a full-height 4×2 grid — five colored folder
/// tiles (Clean/Crunch/Drive/Lead/Ambient) plus Tap Tempo, FX Toggle, and
/// Volume Boost function switches.
#[component]
pub fn PerformGrid(
    model: PerformanceModel,
    on_press: Callback<usize>,
    on_toggle_fx: Callback<()>,
    on_toggle_boost: Callback<()>,
    on_tap_tempo: Callback<()>,
) -> Element {
    let stacks = model.stacks;
    let fx_sub = if model.fx_bypass { "Bypassed" } else { "Active" };
    let boost_sub = if model.boost { "+6 dB" } else { "Off" };
    rsx! {
        div { class: "grid grid-cols-4 grid-rows-2 gap-3 h-full",
            // Folders (positions 1–5): Clean, Crunch, Drive, Lead, Ambient.
            for i in 0..5usize {
                if let Some(stack) = stacks.get(i).cloned() {
                    StackTile { key: "s{i}", index: i, stack, on_press }
                } else {
                    div { key: "s{i}", class: "rounded-xl border-2 border-dashed border-border/30" }
                }
            }
            // Position 6: Tap Tempo (white, blinking at tempo).
            TapTempoTile { tempo_bpm: model.tempo_bpm, on_tap: on_tap_tempo }
            // Position 7: FX Toggle (pink).
            FnTile {
                title: "FX Toggle".to_string(),
                subtitle: fx_sub.to_string(),
                bg: "#ec4899".to_string(),
                text: "#ffffff".to_string(),
                active: model.fx_bypass,
                onclick: on_toggle_fx,
            }
            // Position 8: Volume Boost (white).
            FnTile {
                title: "Boost".to_string(),
                subtitle: boost_sub.to_string(),
                bg: "#fafafa".to_string(),
                text: "#0a0a0a".to_string(),
                active: model.boost,
                onclick: on_toggle_boost,
            }
        }
    }
}

/// One colored footswitch folder tile.
#[component]
fn StackTile(index: usize, stack: PerfStack, on_press: Callback<usize>) -> Element {
    let (bg, text) = folder_color(&stack.name);
    let state_cls = if stack.is_active {
        "ring-4 ring-white/80 shadow-xl opacity-100"
    } else {
        "opacity-[0.22] saturate-50 hover:opacity-60"
    };
    rsx! {
        button {
            class: format!(
                "relative flex flex-col items-center justify-center gap-1 rounded-xl transition-all h-full {state_cls}"
            ),
            style: "background-color: {bg}; color: {text};",
            onclick: move |_| on_press.call(index),
            // Amber dot while the current patch is still loading.
            if !stack.available {
                span { class: "absolute top-2 right-2 w-2.5 h-2.5 rounded-full",
                    style: "background-color: #fde047;" }
            }
            span { class: "text-2xl font-bold tracking-wide", "{stack.name}" }
            span { class: "text-sm font-semibold opacity-90", "{stack.current_patch}" }
            // Rotation dots — one per patch in the folder, current one lit.
            // Reads as "where the next press lands" without any counting.
            if stack.patch_count > 1 {
                div { class: "flex items-center gap-1.5 mt-1",
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
    }
}

/// A function-switch tile (FX Toggle, Volume Boost).
#[component]
fn FnTile(
    title: String,
    subtitle: String,
    bg: String,
    text: String,
    active: bool,
    onclick: Callback<()>,
) -> Element {
    let state_cls = if active {
        "ring-4 ring-white/80 shadow-xl opacity-100"
    } else {
        "opacity-[0.3] saturate-50 hover:opacity-70"
    };
    rsx! {
        button {
            class: format!(
                "flex flex-col items-center justify-center gap-1 rounded-xl transition-all h-full {state_cls}"
            ),
            style: "background-color: {bg}; color: {text};",
            onclick: move |_| onclick.call(()),
            span { class: "text-xl font-bold tracking-wide", "{title}" }
            span { class: "text-xs opacity-80", "{subtitle}" }
        }
    }
}

/// Tap Tempo tile — muted like the other function tiles, with a ring around
/// the block flashing at the current tempo (the tile *is* the metronome).
/// The timer uses `architect::platform::sleep`, so it runs on tokio natively
/// and browser timers on wasm.
#[component]
fn TapTempoTile(tempo_bpm: u32, on_tap: Callback<()>) -> Element {
    let mut lit = use_signal(|| false);
    // Flash the ring on the beat: on for the front edge, off for the rest.
    use_future(move || async move {
        loop {
            let bpm = tempo_bpm.max(40) as u64;
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
    rsx! {
        button {
            class: "relative flex flex-col items-center justify-center gap-1 rounded-xl h-full transition-shadow duration-100 opacity-90 hover:opacity-100",
            style: "background-color: #27272a; color: #d4d4d8; {ring}",
            onclick: move |_| on_tap.call(()),
            span { class: "text-lg font-bold tracking-wide", "Tap Tempo" }
            span { class: "text-[11px] text-zinc-500", "{tempo_bpm} BPM · hold: tuner" }
        }
    }
}
