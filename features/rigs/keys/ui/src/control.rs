//! **Control view** — the mixer.
//!
//! Where the guitar rig's Control view is a pedalboard (FX blocks and their
//! params), the keys rig's is a console: one strip per layer, grouped under
//! its engine, plus an engine trim and the rig master — over the keyboard the
//! patch is spread across. This is the surface a keys player actually
//! performs on — ride the pad under a verse, pull the piano back for a hook,
//! drop the organ out until the last chorus.
//!
//! A strip's two ends are its two verbs: the lane's letter at the top takes it
//! in and out of the patch, and the patch name at the bottom opens the layer
//! (where the Soundsources panel loads a different sound into it).
//!
//! Under the strips sits the **macro band** — the Global Controls of whatever
//! is selected. Click the Keys card and it is the Keys engine's knobs (one
//! Filter, one Envelope, one Ambience over every lane it holds); click a lane
//! and it is that lane's, over its modules. Each level offsets the one below
//! it, so the same three knobs are a broad stroke at the top and a precise
//! edit further in.

use dioxus::prelude::*;
use signal_keys_proto::keys::KeysRigClient;
use signal_keys_proto::{KeysEngineModel, KeysLayerModel, KeysMacro, KeysMixer};

use crate::selection::Selection;
use signal_ui::components::Piano;

use crate::fader::{EdgeFader, Fader, fmt_db};
use crate::zoom::{OpenButton, Zoom};

/// Accent per engine — the same color language the Perform strip uses.
/// Keys is the green one (it is the engine you look at most, and green reads
/// as "running"); the blue belongs to Pad.
pub fn engine_color(name: &str) -> &'static str {
    match name {
        "Keys" => "#34d399",
        // "Aux" is the old Synth engine: everything auxiliary lands there.
        "Aux" | "Synth" => "#a78bfa",
        "Organ" => "#fb923c",
        "Pad" => "#38bdf8",
        // A drone is the bed nothing points at — grey keeps it behind the
        // engines you play. SFX is white: it is a one-shot, not a colour.
        "Drone" => "#9ca3af",
        "SFX" => "#e4e4e7",
        _ => "#94a3b8",
    }
}

/// The mixer, over the keyboard the patch is spread across.
#[component]
pub fn ControlView(
    mixer: KeysMixer,
    /// Notes currently held, for the keyboard at the base.
    #[props(default)]
    held: Vec<u8>,
) -> Element {
    rsx! {
        div { style: "flex: 1; min-height: 0; display: flex; flex-direction: column;",
            // The engine band. Engines and their lanes are both unbounded, so
            // the band scrolls sideways rather than squeezing cards — and the
            // master sits outside the scroller, because the one fader you must
            // always be able to reach is the one that stops the sound.
            div {
                style: "flex: 1; min-height: 0; display: flex; align-items: stretch; \
                        border-bottom: 1px solid #1c1c1f;",
                div {
                    // `space-between` spends the slack on the gaps when the
                    // engines don't fill the width, and gets out of the way
                    // when they overflow (the first card stays flush left and
                    // the row scrolls, which `center`/`space-around` would
                    // break by pushing the start out of reach).
                    style: "flex: 1; min-width: 0; overflow-x: auto; overflow-y: auto; \
                            padding: 12px 22px; display: flex; align-items: flex-start; \
                            justify-content: space-between; gap: 14px;",
                    for engine in mixer.engines.iter() {
                        div {
                            key: "{engine.name}",
                            style: "flex: 0 0 auto;",
                            EngineStrip { engine: engine.clone() }
                        }
                    }
                }
                div {
                    style: "flex: 0 0 auto; display: flex; align-items: flex-start; \
                            padding: 12px; border-left: 1px solid #1c1c1f; background: #0b0b0e;",
                    MasterStrip { master_db: mixer.master_db }
                }
            }
            MacroBand {}
            KeyboardStrip { mixer: mixer.clone(), held }
        }
    }
}

/// Which node's Global Controls the band shows: `(engine, name, is_engine)`.
/// A module selection still shows its LAYER — anything deeper is the zoom's
/// job.
fn scope_of(selection: &Selection) -> Option<(String, String, bool)> {
    match selection {
        Selection::Engine(engine) => Some((engine.clone(), engine.clone(), true)),
        Selection::Layer { engine, layer } | Selection::Module { engine, layer, .. } => {
            Some((engine.clone(), layer.clone(), false))
        }
        Selection::None => None,
    }
}

/// **The macro band** — the selected node's Global Controls, under the strips
/// that select it.
///
/// It follows the selection rather than the zoom: picking a card or a lane is
/// already how the browser is aimed, so it is also how the knobs are. An
/// engine's macros drive every module in every one of its lanes; a lane's
/// drive its own. Both are offsets into the level beneath once that level
/// stops agreeing with itself — the panels say "offset" and print the spread.
#[component]
fn MacroBand() -> Element {
    let rig = use_hook(try_consume_context::<KeysRigClient>);
    let state = use_hook(try_consume_context::<crate::state::KeysViewState>);
    let selection = crate::selection::use_selection();
    let mut macros = use_signal(Vec::<KeysMacro>::new);

    let scope = scope_of(&selection.read());
    let accent = scope
        .as_ref()
        .map(|(engine, ..)| engine_color(engine).to_string())
        .unwrap_or_else(|| "#94a3b8".to_string());

    // Re-pull whenever the selection moves or the rig publishes a mixer —
    // every macro move republishes, so the band reflects what it just did.
    // Both are read INSIDE the effect: that is what makes it re-run, and what
    // keeps it from firing on a selection it captured once at mount.
    {
        let rig = rig.clone();
        use_effect(move || {
            // Track the mixer so a change anywhere refreshes the readouts.
            if let Some(state) = state {
                let _ = state.mixer.read();
            }
            let scope = scope_of(&selection.read());
            let rig = rig.clone();
            spawn(async move {
                let Some(rig) = rig else { return };
                match scope {
                    Some((_, name, true)) => {
                        if let Ok(d) = rig.engine_detail(name).await {
                            macros.set(d.macros);
                        }
                    }
                    Some((_, name, false)) => {
                        if let Ok(d) = rig.layer_detail(name, 0).await {
                            macros.set(d.layer_macros);
                        }
                    }
                    None => macros.set(Vec::new()),
                }
            });
        });
    }

    let (label, level) = match &scope {
        Some((_, name, true)) => (name.clone(), "engine controls".to_string()),
        Some((_, name, false)) => (name.clone(), "layer controls".to_string()),
        None => (String::new(), String::new()),
    };
    let items = macros.read().clone();

    rsx! {
        div {
            style: "flex-shrink: 0; display: flex; flex-direction: column; gap: 8px; \
                    padding: 10px 12px; border-top: 1px solid #1c1c1f; background: #0a0a0c;",
            div { style: "display: flex; align-items: baseline; gap: 8px;",
                if label.is_empty() {
                    span { style: "font-size: 10px; color: #52525b;",
                        "Pick an engine or a lane to control it from here."
                    }
                } else {
                    span { style: "font-size: 11px; font-weight: 700; color: {accent};", "{label}" }
                    span {
                        style: "font-size: 9px; font-weight: 700; letter-spacing: 0.1em; \
                                text-transform: uppercase; color: #52525b;",
                        "{level}"
                    }
                }
            }
            if !items.is_empty() {
                crate::macro_panel::MacroPanel {
                    macros: items,
                    accent: accent.clone(),
                    on_change: {
                        let (rig, scope) = (rig.clone(), scope.clone());
                        move |(id, v): (String, f32)| {
                            let (rig, scope) = (rig.clone(), scope.clone());
                            spawn(async move {
                                let Some(rig) = rig else { return };
                                match scope {
                                    Some((_, name, true)) => {
                                        let _ = rig.set_engine_global(name, id, v).await;
                                    }
                                    Some((_, name, false)) => {
                                        let _ = rig.set_layer_global(name, id, v).await;
                                    }
                                    None => {}
                                }
                            });
                        }
                    },
                }
            }
        }
    }
}

/// **The keyboard** — the patch as the player meets it: one band per audible
/// lane across its key window, over a keyboard lit by what is being held.
///
/// The mixer says how loud each lane is; this says *where* it is. Splits and
/// zones land here as the mapping surface grows.
#[component]
fn KeyboardStrip(mixer: KeysMixer, held: Vec<u8>) -> Element {
    const LO: u8 = 21;
    const HI: u8 = 108;
    let rig = use_hook(try_consume_context::<KeysRigClient>);

    // Every audible lane's key window, engine-coloured.
    let bands: Vec<(String, String, String, f32, f32)> = mixer
        .engines
        .iter()
        .flat_map(|e| {
            let color = engine_color(&e.name).to_string();
            let engine = e.name.clone();
            e.layers.iter().filter(|l| l.live && !l.muted).map(move |l| {
                let lo = (l.key_lo as u8).clamp(LO, HI);
                let hi = (l.key_hi as u8).clamp(LO, HI);
                (
                    lane_letter(&l.name, &engine),
                    if l.patch.is_empty() { l.name.clone() } else { l.patch.clone() },
                    color.clone(),
                    white_fraction(lo, false),
                    white_fraction(hi, true),
                )
            })
        })
        .collect();

    rsx! {
        div {
            style: "flex-shrink: 0; display: flex; flex-direction: column; gap: 6px; \
                    padding: 10px 12px 12px; border-top: 1px solid #1c1c1f; background: #0b0b0e;",
            // Split bands, aligned to the keys beneath them.
            div { style: "position: relative; height: {14 * bands.len().max(1)}px;",
                for (i, (letter, patch, color, from, to)) in bands.iter().enumerate() {
                    div {
                        key: "{i}-{letter}",
                        style: format!(
                            "position: absolute; top: {}px; left: {:.3}%; width: {:.3}%; height: 12px; \
                             display: flex; align-items: center; gap: 5px; padding: 0 6px; \
                             border-radius: 3px; background: {}22; border-left: 2px solid {}; \
                             overflow: hidden; white-space: nowrap;",
                            i * 14,
                            from * 100.0,
                            (to - from) * 100.0,
                            color,
                            color,
                        ),
                        span { style: "font-size: 8px; font-weight: 800; color: {color};", "{letter}" }
                        span { style: "font-size: 8px; color: #71717a; text-overflow: ellipsis; overflow: hidden;",
                            "{patch}"
                        }
                    }
                }
            }
            Piano {
                start_note: LO,
                end_note: HI,
                active_notes: held,
                show_labels: false,
                waterfall: false,
                accent_color: "#a78bfa".to_string(),
                height: "96px",
                on_note_on: {
                    let rig = rig.clone();
                    move |n: u8| {
                        let rig = rig.clone();
                        spawn(async move {
                            if let Some(r) = rig { let _ = r.trigger(n as u32, 100).await; }
                        });
                    }
                },
                on_note_off: {
                    let rig = rig.clone();
                    move |n: u8| {
                        let rig = rig.clone();
                        spawn(async move {
                            if let Some(r) = rig { let _ = r.trigger(n as u32, 0).await; }
                        });
                    }
                },
            }
        }
    }
}

/// Where a note sits across the keyboard, 0..1, counted in **white keys** —
/// the piano lays white keys out evenly and hangs the black ones between, so
/// counting note numbers would drift a band off its keys by up to a fifth.
/// `past` measures to the far edge of the note's key rather than its near one.
fn white_fraction(note: u8, past: bool) -> f32 {
    fn is_white(n: u8) -> bool {
        !matches!(n % 12, 1 | 3 | 6 | 8 | 10)
    }
    let whites = |upto: u8| (21..upto).filter(|n| is_white(*n)).count() as f32;
    let total = whites(109);
    let n = note.clamp(21, 108);
    let at = whites(if past { n + 1 } else { n });
    (at / total).clamp(0.0, 1.0)
}

/// One engine: **its level is the card's left edge**, its layers sit beside it,
/// and the lamp by its name bypasses the whole engine.
///
/// The engine level used to be a labelled row across the bottom of the card,
/// which read as a fourth control per engine and cost a whole band of height.
/// It is a trim you set once, not a lane you ride — so it became the outline.
#[component]
fn EngineStrip(engine: KeysEngineModel) -> Element {
    let rig = use_hook(try_consume_context::<KeysRigClient>);
    let mut zoom = crate::zoom::use_zoom();
    let mut selection = crate::selection::use_selection();
    let picked = *selection.read() == Selection::Engine(engine.name.clone());
    let pick_name = engine.name.clone();
    let accent = engine_color(&engine.name);
    let muted = engine.muted;
    let name = engine.name.clone();
    let open_name = engine.name.clone();
    let dbl_name = engine.name.clone();

    rsx! {
        div {
            style: format!(
                "position: relative; display: flex; padding: 10px 10px 10px 20px; cursor: pointer; \
                 border: 1px solid {}; border-radius: 12px; background: {};",
                if picked { accent } else if muted { "#1f1f23" } else { "#26262d" },
                if picked { "#101216" } else { "#0e0e11" },
            ),
            // Clicking the card body — anywhere that is not a control — points
            // the browser at this engine. Double-click zooms into it.
            onclick: move |_| selection.set(Selection::Engine(pick_name.clone())),
            ondoubleclick: move |_| zoom.set(Zoom::Engine(dbl_name.clone())),
            // The engine's level IS the card's left edge.
            EdgeFader {
                db: engine.gain_db,
                accent: accent.to_string(),
                dimmed: muted,
                on_change: {
                    let rig = rig.clone();
                    let name = name.clone();
                    move |db: f32| {
                        let (rig, name) = (rig.clone(), name.clone());
                        spawn(async move {
                            if let Some(r) = rig {
                                let _ = r.set_engine_gain(name, db).await;
                            }
                        });
                    }
                },
            }
            div { style: "display: flex; flex-direction: column; gap: 10px; min-width: 0;",
                // Engine header: the bypass lamp, the name, the way in.
                div { style: "display: flex; align-items: center; gap: 8px;",
                    button {
                        style: format!(
                            "appearance: none; width: 16px; height: 16px; border-radius: 999px; \
                             cursor: pointer; padding: 0; border: 2px solid {}; background: {}; \
                             box-shadow: {};",
                            if muted { "#3f3f46" } else { accent },
                            if muted { "#131316" } else { accent },
                            if muted { "none".to_string() } else { format!("0 0 8px {accent}99") },
                        ),
                        title: if muted { "{engine.name} — bypassed, click to enable" } else { "{engine.name} — on, click to bypass" },
                        onclick: {
                            let rig = rig.clone();
                            let name = name.clone();
                            move |_| {
                                let (rig, name) = (rig.clone(), name.clone());
                                spawn(async move {
                                    if let Some(r) = rig {
                                        let _ = r.set_engine_mute(name, !muted).await;
                                    }
                                });
                            }
                        },
                    }
                    span {
                        style: format!(
                            "font-size: 12px; font-weight: 700; color: {};",
                            if muted { "#52525b" } else { "#e4e4e7" },
                        ),
                        "{engine.name}"
                    }
                    div { style: "flex: 1;" }
                    OpenButton {
                        title: format!("Open {}", engine.name),
                        on_open: move |_| zoom.set(Zoom::Engine(open_name.clone())),
                    }
                }
                // Layer strips.
                div { style: "display: flex; gap: 10px; align-items: flex-start;",
                    for layer in engine.layers.iter() {
                        LayerStrip {
                            key: "{layer.name}",
                            layer: layer.clone(),
                            accent: accent.to_string(),
                        }
                    }
                }
            }
        }
    }
}

/// A layer's short name — "Keys A" under the Keys engine is just **A**. The
/// engine is already named on the card above it.
fn lane_letter(lane: &str, engine: &str) -> String {
    lane.strip_prefix(engine)
        .map(|rest| rest.trim().to_string())
        .filter(|rest| !rest.is_empty())
        .unwrap_or_else(|| lane.to_string())
}

/// One layer lane, top to bottom: **the lane's letter as its on/off switch**,
/// the fader with mute/solo stacked beside it, and **what it is playing**
/// (click to open the layer).
///
/// The two ends are the two things a player does mid-set: drop a layer out, or
/// open its sound to shape it. Dropping it out is the one done mid-phrase with
/// one hand, so the letter takes the top — where a hand lands without looking.
#[component]
fn LayerStrip(layer: KeysLayerModel, accent: String) -> Element {
    let rig = use_hook(try_consume_context::<KeysRigClient>);
    let mut zoom = crate::zoom::use_zoom();
    let mut selection = crate::selection::use_selection();
    let picked = matches!(
        &*selection.read(),
        Selection::Layer { layer: l, .. } | Selection::Module { layer: l, .. } if *l == layer.name
    );
    let pick = Selection::Layer { engine: layer.engine.clone(), layer: layer.name.clone() };
    let open_lane = layer.name.clone();
    let dbl_lane = layer.name.clone();
    let patch_label = if layer.patch.is_empty() { "empty".to_string() } else { layer.patch.clone() };
    let letter = lane_letter(&layer.name, &layer.engine);
    let split = if layer.key_lo == 0 && layer.key_hi == 127 {
        String::new()
    } else {
        format!("{}–{}", note_name(layer.key_lo), note_name(layer.key_hi))
    };
    // "On" is "not muted": the rig has one audible/silent state per lane, and
    // the letter and the M button are two sizes of the same switch.
    let on = !layer.muted;

    rsx! {
        div {
            style: format!(
                "position: relative; display: flex; flex-direction: column; align-items: stretch; \
                 gap: 8px; width: 124px; padding: 6px; border-radius: 10px; cursor: pointer; \
                 background: {}; border: 1px solid {};",
                if picked { "#101216" } else { "transparent" },
                if picked { accent.clone() } else { "transparent".to_string() },
            ),
            // Picking the lane points the browser at it; the letter and the
            // patch name keep their own jobs (they stop propagation below).
            onclick: move |e: MouseEvent| {
                e.stop_propagation();
                selection.set(pick.clone());
            },
            ondoubleclick: move |e: MouseEvent| {
                e.stop_propagation();
                zoom.set(Zoom::Layer(dbl_lane.clone()));
            },
            // The lane's letter IS its on/off — the top of the strip, where a
            // hand lands without looking.
            button {
                style: format!(
                    "appearance: none; border: 1px solid {}; border-radius: 8px; cursor: pointer; \
                     padding: 7px 0; font-size: 14px; font-weight: 800; letter-spacing: 0.06em; \
                     background: {}; color: {};",
                    if on && layer.live { accent.clone() } else { "#26262b".to_string() },
                    if on && layer.live { "#101821".to_string() } else { "#0f0f12".to_string() },
                    if on && layer.live {
                        accent.clone()
                    } else if on {
                        "#52525b".to_string()
                    } else {
                        "#3f3f46".to_string()
                    },
                ),
                title: if on { "{layer.name} — on" } else { "{layer.name} — off" },
                onclick: {
                    let rig = rig.clone();
                    let lane = layer.name.clone();
                    let muted = layer.muted;
                    move |_| {
                        let (rig, lane) = (rig.clone(), lane.clone());
                        spawn(async move {
                            if let Some(r) = rig {
                                let _ = r.set_layer_mute(lane, !muted).await;
                            }
                        });
                    }
                },
                "{letter}"
            }
            // Fader, with mute and solo stacked down its right.
            div { style: "display: flex; align-items: flex-start; gap: 8px; justify-content: center;",
                Fader {
                    db: layer.gain_db,
                    height_px: 120,
                    accent: accent.clone(),
                    dimmed: layer.muted || !layer.live,
                    on_change: {
                        let rig = rig.clone();
                        let lane = layer.name.clone();
                        move |db: f32| {
                            let (rig, lane) = (rig.clone(), lane.clone());
                            spawn(async move {
                                if let Some(r) = rig {
                                    let _ = r.set_layer_gain(lane, db).await;
                                }
                            });
                        }
                    },
                }
                div { style: "display: flex; flex-direction: column; gap: 4px;",
                    button {
                        style: mute_style(layer.muted),
                        title: "Mute",
                        onclick: {
                            let rig = rig.clone();
                            let lane = layer.name.clone();
                            let muted = layer.muted;
                            move |_| {
                                let (rig, lane) = (rig.clone(), lane.clone());
                                spawn(async move {
                                    if let Some(r) = rig {
                                        let _ = r.set_layer_mute(lane, !muted).await;
                                    }
                                });
                            }
                        },
                        "M"
                    }
                    button {
                        style: solo_style(layer.soloed),
                        title: "Solo",
                        onclick: {
                            let rig = rig.clone();
                            let lane = layer.name.clone();
                            let soloed = layer.soloed;
                            move |_| {
                                let (rig, lane) = (rig.clone(), lane.clone());
                                spawn(async move {
                                    if let Some(r) = rig {
                                        let _ = r.set_layer_solo(lane, !soloed).await;
                                    }
                                });
                            }
                        },
                        "S"
                    }
                }
            }
            // What it is playing — and the way in.
            button {
                style: format!(
                    "appearance: none; border: 1px solid {}; border-radius: 8px; cursor: pointer; \
                     padding: 7px 8px; background: {}; color: {}; text-align: left; \
                     display: flex; flex-direction: column; gap: 2px; min-height: 44px;",
                    if layer.live { "#2b3a4d" } else { "#26262b" },
                    if layer.live { "#101821" } else { "#131316" },
                    if layer.live { "#7dd3fc" } else { "#52525b" },
                ),
                title: "Open {layer.name}",
                onclick: move |_| zoom.set(Zoom::Layer(open_lane.clone())),
                span {
                    style: "font-size: 11px; font-weight: 600; line-height: 1.25; \
                            overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                    "{patch_label}"
                }
                if !split.is_empty() {
                    span { style: "font-size: 8px; color: #52525b;", "{split}" }
                }
            }
        }
    }
}

/// The rig master strip.
#[component]
fn MasterStrip(master_db: f32) -> Element {
    let rig = use_hook(try_consume_context::<KeysRigClient>);
    rsx! {
        div {
            style: "display: flex; flex-direction: column; align-items: center; gap: 8px; padding: 10px 14px; \
                    border: 1px solid #1f1f23; border-radius: 12px; background: #0e0e11;",
            span { style: "font-size: 11px; font-weight: 700; color: #e4e4e7;", "Master" }
            Fader {
                db: master_db,
                height_px: 160,
                accent: "#e4e4e7".to_string(),
                on_change: move |db: f32| {
                    let rig = rig.clone();
                    spawn(async move {
                        if let Some(r) = rig {
                            let _ = r.set_master_gain(db).await;
                        }
                    });
                },
            }
            span { style: "font-size: 9px; color: #52525b;", {fmt_db(master_db)} }
        }
    }
}

fn mute_style(on: bool) -> String {
    format!(
        "appearance: none; border: 1px solid {}; border-radius: 5px; width: 24px; height: 20px; \
         background: {}; color: {}; font-size: 9px; font-weight: 700;",
        if on { "#7f1d1d" } else { "#26262b" },
        if on { "#3f1414" } else { "#131316" },
        if on { "#fca5a5" } else { "#52525b" },
    )
}

fn solo_style(on: bool) -> String {
    format!(
        "appearance: none; border: 1px solid {}; border-radius: 5px; width: 24px; height: 20px; \
         background: {}; color: {}; font-size: 9px; font-weight: 700;",
        if on { "#a16207" } else { "#26262b" },
        if on { "#3b2708" } else { "#131316" },
        if on { "#fde047" } else { "#52525b" },
    )
}

/// MIDI note → name, for key-split labels.
fn note_name(note: u32) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let n = note as i32;
    format!("{}{}", NAMES[(n % 12) as usize], n / 12 - 1)
}
