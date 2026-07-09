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
use dioxus::dioxus_core::Task;

use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::LiveBlock;
use signal_guitar_proto::{PerfStack, PerformanceModel, TunerReading};

/// How long a press must last to count as a hold (footswitch convention).
const HOLD_MS: u64 = 500;

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

/// The physical switch number, pinned to a tile corner.
#[component]
fn SwitchNo(no: usize) -> Element {
    rsx! {
        span { class: "absolute top-1.5 left-2.5 text-[10px] font-mono opacity-40", "{no}" }
    }
}

/// A footswitch-shaped button with the tap/hold split every switch shares:
/// press-and-release fires `on_tap`; holding for [`HOLD_MS`] fires `on_hold`
/// instead (release then does nothing). Pointer events, so it behaves the
/// same with a mouse or a finger on a stage tablet. No `on_hold` → every
/// press is a tap, however long.
#[component]
fn HoldButton(
    class: String,
    style: String,
    on_tap: Callback<()>,
    #[props(default)] on_hold: Option<Callback<()>>,
    children: Element,
) -> Element {
    let mut hold_fired = use_signal(|| false);
    let mut hold_task = use_signal(|| None::<Task>);
    rsx! {
        button {
            class: "{class}",
            style: "{style}",
            onpointerdown: move |_| {
                hold_fired.set(false);
                if let Some(hold) = on_hold {
                    let task = spawn(async move {
                        architect::platform::sleep(Duration::from_millis(HOLD_MS)).await;
                        hold_fired.set(true);
                        hold.call(());
                    });
                    hold_task.set(Some(task));
                }
            },
            onpointerup: move |_| {
                if let Some(task) = hold_task.take() {
                    task.cancel();
                }
                if !hold_fired() {
                    on_tap.call(());
                }
            },
            onpointerleave: move |_| {
                // Dragging off the switch cancels the press entirely.
                if let Some(task) = hold_task.take() {
                    task.cancel();
                }
            },
            {children}
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
) -> Element {
    let stacks = model.stacks;
    let rig = use_hook(try_consume_context::<RigClient>);
    let mode = model.perform_mode;
    // Preset mode browses the pool — fetch it whenever the model moves.
    let presets = use_resource({
        let rig = rig.clone();
        let rev = model.revision;
        move || {
            let _ = rev;
            let rig = rig.clone();
            async move {
                match rig {
                    Some(r) => r.presets().await.unwrap_or_default(),
                    None => Vec::new(),
                }
            }
        }
    });
    let preset_list = presets.read().clone().unwrap_or_default();
    // Pedal view: the active chain's toggleable FX as stomp switches.
    let chain = use_resource({
        let rig = rig.clone();
        let rev = model.revision;
        move || {
            let _ = rev;
            let rig = rig.clone();
            async move {
                match rig {
                    Some(r) => r.chain().await.unwrap_or_default(),
                    None => Vec::new(),
                }
            }
        }
    });
    let chain_blocks = chain.read().clone().unwrap_or_default();
    // The pedalboard: comp + drives + boost on the top row, time + mod on
    // the bottom — matching the two-row switch layout.
    let pedal_types = |b: &LiveBlock| {
        use signal_proto::block::BlockType as B;
        matches!(
            b.block_type,
            B::Compressor | B::Drive | B::Boost | B::Delay | B::Reverb
                | B::Chorus | B::Flanger | B::Phaser | B::Trem | B::Vibrato | B::Rotary
        )
    };
    let pedals: Vec<LiveBlock> = chain_blocks.iter().filter(|b| pedal_types(b)).cloned().collect();
    let set_mode = {
        let rig = rig.clone();
        move |m: u32| {
            if let Some(r) = rig.clone() {
                spawn(async move { let _ = r.set_perform_mode(m).await; });
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
        Some(Callback::new(move |_: ()| on_press.call(4))),
        Some(Callback::new(move |_: ()| on_toggle_fx.call(()))),
        Some(Callback::new(move |_: ()| song_layer.set(true))),
        Some(Callback::new(move |_: ()| on_toggle_boost.call(()))),
    ];

    let current_song = model
        .songs
        .get(model.song_index as usize)
        .map(|s| s.name.clone())
        .unwrap_or_default();
    let song_pos = format!("{}/{}", model.song_index + 1, model.songs.len().max(1));

    rsx! {
        div { class: "flex flex-col h-full min-h-0 gap-2",
        // Mode switcher: Preset (pool browsing) · Profile (stacks) ·
        // Setlist (song-adaptive parts + stacks).
        div { class: "flex items-center gap-1 flex-shrink-0",
            for (m, label) in [(0u32, "Preset"), (1, "Profile"), (2, "Setlist")] {
                button {
                    key: "{m}",
                    class: if mode == m {
                        "px-3 py-1 rounded-md text-xs font-bold bg-accent text-accent-foreground"
                    } else {
                        "px-3 py-1 rounded-md text-xs text-muted-foreground border border-border hover:bg-accent/40"
                    },
                    onclick: {
                        let set_mode = set_mode.clone();
                        move |_| set_mode(m)
                    },
                    "{label}"
                }
            }
            if mode == 2 {
                span { class: "ml-2 text-xs text-muted-foreground truncate", "{current_song} · {song_pos}" }
            }
        }

        if mode == 0 {
            // ── Pedal view: the active chain's FX as stomp switches; pick
            // the base preset from the bar. Tap = bypass toggle (auto-saved
            // into the patch like any live edit) ──
            div { class: "flex items-center gap-2 flex-shrink-0",
                span { class: "text-[10px] uppercase tracking-wider text-muted-foreground", "Preset" }
                select {
                    class: "bg-background border border-border rounded px-2 py-1 text-sm",
                    onchange: {
                        let rig = rig.clone();
                        move |e: FormEvent| {
                            if let (Some(r), Ok(i)) = (rig.clone(), e.value().parse::<u32>()) {
                                spawn(async move { let _ = r.play_preset(i).await; });
                            }
                        }
                    },
                    for (i, p) in preset_list.iter().enumerate() {
                        option { key: "{i}", value: "{i}", selected: p.active, "{p.name}" }
                    }
                }
            }
            div { class: "grid grid-cols-5 auto-rows-fr gap-3 flex-1 min-h-0",
                for b in pedals.iter() {
                    {
                        let on = !b.bypassed;
                        let id = b.id.clone();
                        let color = b.block_type.color().border.to_string();
                        rsx! {
                            button {
                                key: "{b.id}",
                                class: "rounded-2xl border flex flex-col items-center justify-center gap-2 p-3 transition-all duration-100",
                                style: if on {
                                    format!("border-color: {color}; background: color-mix(in srgb, {color} 22%, #0a0a0a); box-shadow: inset 0 0 40px color-mix(in srgb, {color} 12%, transparent);")
                                } else {
                                    "border-color: #27272a; background: #0f0f10; opacity: 0.75;".to_string()
                                },
                                onclick: {
                                    let rig = rig.clone();
                                    move |_| {
                                        let id = id.clone();
                                        if let Some(r) = rig.clone() {
                                            spawn(async move { let _ = r.toggle_block_bypass(id).await; });
                                        }
                                    }
                                },
                                // Pedal LED.
                                span {
                                    class: "w-3 h-3 rounded-full",
                                    style: if on {
                                        format!("background-color: {color}; box-shadow: 0 0 8px {color};")
                                    } else {
                                        "background-color: #3f3f46;".to_string()
                                    },
                                }
                                span { class: "text-base font-bold text-center leading-tight", "{b.name}" }
                                if !b.preset.is_empty() {
                                    span { class: "text-[10px] opacity-60 truncate max-w-full", "{b.preset}" }
                                }
                                // Editing surface: block presets with NAM
                                // options get a picker right on the pedal.
                                if !b.options.is_empty() {
                                    select {
                                        class: "bg-background/80 border border-border rounded px-1 py-0.5 text-[10px] max-w-full",
                                        onclick: move |e: MouseEvent| e.stop_propagation(),
                                        onchange: {
                                            let rig = rig.clone();
                                            let id = b.id.clone();
                                            move |e: FormEvent| {
                                                if let (Some(r), Ok(i)) = (rig.clone(), e.value().parse::<u32>()) {
                                                    let id = id.clone();
                                                    spawn(async move { let _ = r.set_block_option(id, i).await; });
                                                }
                                            }
                                        },
                                        for (i, opt) in b.options.iter().enumerate() {
                                            option { key: "{i}", value: "{i}", selected: i as u32 == b.option, "{opt}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else {
        // Hold layer ABOVE the main switches (a footswitch's hold function
        // lives "up" from your toe) — a slim strip, ~1/8 the main row.
        div {
            class: "grid grid-cols-5 gap-3 flex-1 min-h-0",
            style: "grid-template-rows: minmax(44px, 1fr) minmax(0, 7fr);",

            // ── Row A: setlist mode shows the song's parts; otherwise the
            // hold layer (switches 6–10), compact ──
            if mode == 2 && !model.parts.is_empty() {
                for (i, part) in model.parts.iter().enumerate() {
                    button {
                        key: "part-{i}",
                        class: if i == model.part_index as usize {
                            "rounded-lg px-2 text-sm font-bold bg-accent text-accent-foreground min-h-0"
                        } else {
                            "rounded-lg px-2 text-sm text-muted-foreground border border-border hover:bg-accent/40 min-h-0"
                        },
                        onclick: {
                            let rig = rig.clone();
                            move |_| {
                                if let Some(r) = rig.clone() {
                                    spawn(async move { let _ = r.select_part(i as u32).await; });
                                }
                            }
                        },
                        "{part}"
                    }
                }
            } else if let Some(stack) = stacks.get(4).cloned() {
                StackTile { index: 4usize, switch_no: 6, stack, on_press, compact: true }
            } else {
                div { class: "relative rounded-lg border border-dashed border-border/30",
                    SwitchNo { no: 6 }
                }
            }
            // Switch 7 (hold 2): FX Toggle — lit while the Time FX are ON.
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
            // Switch 8 (hold 3): the Song layer — setlist prev/next + fast
            // scroll. Tap toggles the layer; the tile names where you are.
            FnTile {
                title: "Song".to_string(),
                subtitle: format!("{song_pos} · {current_song}"),
                bg: "#a78bfa".to_string(),
                text: "#1e1b4b".to_string(),
                active: song_layer(),
                switch_no: 8,
                compact: true,
                onclick: Callback::new(move |_: ()| song_layer.toggle()),
            }
            // Switch 9 (hold 4): Boost — tap on/off, hold rotates the level.
            BoostTile {
                subtitle: boost_sub,
                active: model.boost_db != 0.0,
                switch_no: 9,
                on_toggle: on_toggle_boost,
                on_cycle: on_cycle_boost,
            }
            // Switch 10 (hold 5): the live tuner, right in the tile.
            LiveTunerTile {
                switch_no: 10,
                onclick: Callback::new({
                    let rig = rig.clone();
                    move |_: ()| {
                        if let Some(r) = rig.clone() {
                            spawn(async move { let _ = r.toggle_tuner().await; });
                        }
                    }
                }),
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
                    on_tap: Callback::new(move |_: ()| song_layer.set(false)),
                    SwitchNo { no: 5 }
                    span { class: "text-xl font-bold", "Back" }
                    span { class: "text-xs text-muted-foreground", "to the rig" }
                }
            } else {
            for i in 0..4usize {
                if let Some(stack) = stacks.get(i).cloned() {
                    StackTile {
                        key: "s{i}",
                        index: i,
                        switch_no: i + 1,
                        stack,
                        on_press,
                        on_hold: hold_actions[i],
                    }
                } else {
                    div { key: "s{i}", class: "relative rounded-xl border-2 border-dashed border-border/30",
                        SwitchNo { no: i + 1 }
                    }
                }
            }
            // Switch 5: Tap Tempo (hold: tuner).
            TapTempoTile {
                tempo_bpm: model.tempo_bpm,
                on_tap: on_tap_tempo,
                on_hold: Callback::new({
                    let rig = rig.clone();
                    move |_: ()| {
                        if let Some(r) = rig.clone() {
                            spawn(async move { let _ = r.toggle_tuner().await; });
                        }
                    }
                }),
            }
            }
        }
        }
        }
    }
}

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
) -> Element {
    let (bg, text) = folder_color(&stack.name);
    let state_cls = if stack.is_active {
        "ring-2 ring-white/80 shadow-xl opacity-100"
    } else {
        "opacity-[0.22] saturate-50 hover:opacity-60"
    };
    let layout_cls = if compact {
        "relative flex items-center justify-center gap-2 rounded-lg"
    } else {
        "relative flex flex-col items-center justify-center gap-1 rounded-xl"
    };
    rsx! {
        HoldButton {
            class: format!("{layout_cls} transition-all h-full {state_cls}"),
            style: format!("background-color: {bg}; color: {text};"),
            on_tap: Callback::new(move |_: ()| on_press.call(index)),
            on_hold,
            SwitchNo { no: switch_no }
            // Amber dot while the current patch is still loading.
            if !stack.available {
                span { class: "absolute top-2 right-2 w-2.5 h-2.5 rounded-full",
                    style: "background-color: #fde047;" }
            }
            span {
                class: if compact { "text-sm font-bold tracking-wide" } else { "text-2xl font-bold tracking-wide" },
                "{stack.name}"
            }
            span {
                class: if compact { "text-[10px] font-semibold opacity-80" } else { "text-sm font-semibold opacity-90" },
                "{stack.current_patch}"
            }
            // The preset this patch points at + which modules it overrides.
            if !compact {
                div { class: "flex items-center gap-1.5",
                    span { class: "text-[10px] font-mono opacity-60", "{stack.preset}" }
                    for m in stack.override_modules.iter() {
                        span {
                            key: "{m}",
                            class: "text-[10px] opacity-80",
                            title: "overrides {m}",
                            {crate::icons::module_icon(m)}
                        }
                    }
                }
            } else if !stack.override_modules.is_empty() {
                span { class: "text-[10px] opacity-70",
                    {stack.override_modules.iter().map(|m| crate::icons::module_icon(m)).collect::<String>()}
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
    let state_cls = if active {
        "ring-2 ring-white/80 shadow-xl opacity-100"
    } else {
        "opacity-[0.3] saturate-50 hover:opacity-70"
    };
    let layout_cls = if compact {
        "relative flex items-center justify-center gap-2 rounded-lg"
    } else {
        "relative flex flex-col items-center justify-center gap-1 rounded-xl"
    };
    rsx! {
        button {
            class: format!("{layout_cls} transition-all h-full {state_cls}"),
            style: "background-color: {bg}; color: {text};",
            onclick: move |_| onclick.call(()),
            SwitchNo { no: switch_no }
            span {
                class: if compact { "text-sm font-bold tracking-wide" } else { "text-xl font-bold tracking-wide" },
                "{title}"
            }
            if !subtitle.is_empty() {
                span {
                    class: if compact { "text-[10px] opacity-80" } else { "text-xs opacity-80" },
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
    let state_cls = if active {
        "ring-2 ring-white/80 shadow-xl opacity-100"
    } else {
        "opacity-[0.3] saturate-50 hover:opacity-70"
    };
    rsx! {
        HoldButton {
            class: format!(
                "relative flex items-center justify-center gap-2 rounded-lg transition-all h-full {state_cls}"
            ),
            style: "background-color: #fafafa; color: #0a0a0a;".to_string(),
            on_tap: on_toggle,
            on_hold: Some(on_cycle),
            SwitchNo { no: switch_no }
            span { class: "text-sm font-bold tracking-wide", "Boost" }
            span { class: "text-[10px] opacity-80", "{subtitle}" }
        }
    }
}

/// Tap Tempo tile — muted like the other function tiles, with a ring around
/// the block flashing at the current tempo (the tile *is* the metronome).
/// Tap = tempo tap; hold = open the tuner (footswitch 5's hold layer).
#[component]
fn TapTempoTile(tempo_bpm: u32, on_tap: Callback<()>, on_hold: Callback<()>) -> Element {
    let mut lit = use_signal(|| false);

    // Props aren't reactive — mirror the tempo into a signal so the blink
    // loop restarts the moment a tap changes it.
    let mut bpm_sig = use_signal(|| tempo_bpm);
    if bpm_sig() != tempo_bpm {
        bpm_sig.set(tempo_bpm);
    }
    // Flash the ring on the beat: on for the front edge, off for the rest.
    // `use_resource` re-runs (dropping the old loop) when `bpm_sig` changes.
    let _blink = use_resource(move || async move {
        let bpm = bpm_sig().max(40) as u64;
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
    rsx! {
        HoldButton {
            class: "relative flex flex-col items-center justify-center gap-1 rounded-xl h-full transition-shadow duration-100 opacity-90 hover:opacity-100".to_string(),
            style: format!("background-color: #27272a; color: #d4d4d8; {ring}"),
            on_tap,
            on_hold: Some(on_hold),
            SwitchNo { no: 5 }
            span { class: "text-lg font-bold tracking-wide", "Tap Tempo" }
            span { class: "text-[11px] text-zinc-500", "{tempo_bpm} BPM · hold: tuner" }
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
        let rig = rig.clone();
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
            class: "relative flex items-center gap-2 rounded-lg px-2 text-left transition-all duration-100 min-h-0 overflow-hidden",
            style: if in_tune {
                "background-color: #14532d; border: 1px solid #22c55e;"
            } else {
                "background-color: #1c2e26; border: 1px solid #2a4438;"
            },
            onclick: move |_| onclick.call(()),
            span { class: "absolute top-0.5 left-1.5 text-[10px] font-mono opacity-40", "{switch_no}" }
            span {
                class: "text-base font-bold w-7 text-center leading-none flex-shrink-0",
                style: if in_tune { "color: #22c55e;" } else if r.active { "color: #e4e4e7;" } else { "color: #4b5563;" },
                if r.active { "{r.note}" } else { "♪" }
            }
            div { class: "relative flex-1 h-3 min-w-0",
                div { class: "absolute inset-x-0 top-1/2 h-px bg-white/20" }
                div { class: "absolute left-1/2 top-0 bottom-0 w-px bg-white/40" }
                if r.active {
                    div {
                        class: "absolute top-0 bottom-0 w-0.5 rounded transition-all duration-75",
                        style: if in_tune { "left: {needle}%; background-color: #22c55e;" } else { "left: {needle}%; background-color: #eab308;" },
                    }
                }
            }
            if r.active {
                span { class: "text-[9px] font-mono opacity-70 flex-shrink-0", {format!("{:+.0}", r.cents)} }
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
        let rig = rig.clone();
        use_future(move || {
            let rig = rig.clone();
            async move {
                let Some(rig) = rig else { return };
                loop {
                    if let Ok(r) = rig.tuner().await {
                        if r.active {
                            let prev = *smooth_cents.peek();
                            smooth_cents.set(prev + (r.cents - prev) * 0.35);
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
                        class: "absolute top-0 bottom-0 w-1 rounded transition-all duration-75",
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
