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

use crate::graphs::{Adsr, ModuleCurve};

use crate::selection::Selection;
use signal_ui::components::Piano;

use crate::fader::{EdgeFader, Fader, fmt_db};
use crate::meter::{EdgeMeter, peak_of};
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
    /// Wheel + pedal positions, for the strip beside the keys.
    #[props(default)]
    controllers: crate::controllers::Controllers,
) -> Element {
    let mut selection = crate::selection::use_selection();
    // Escape clears the selection from wherever the pointer happens to be —
    // the keyboard shortcut for "I am done editing that".
    use_future(move || async move {
        let mut esc = dioxus::document::eval(
            "window.addEventListener('keydown', e => { \
                 if (e.key === 'Escape') { dioxus.send(true); } \
             });",
        );
        while esc.recv::<bool>().await.is_ok() {
            selection.set(Selection::None);
        }
    });

    rsx! {
        div {
            style: "flex: 1; min-height: 0; display: flex; flex-direction: column;",
            // Clicking the surface itself puts the selection down. Cards and
            // strips stop propagation, so only genuinely empty space lands
            // here.
            onclick: move |_| selection.set(Selection::None),
            // The engine band. Engines and their lanes are both unbounded, so
            // the band scrolls sideways rather than squeezing cards — and the
            // master sits outside the scroller, because the one fader you must
            // always be able to reach is the one that stops the sound.
            div {
                // Sized to the cards, not to the window: `flex: 1` here made
                // the section (and the master's column beside it) run all the
                // way down to the macro band. It still shrinks and scrolls
                // when the cards are taller than the room available.
                style: "flex: 0 1 auto; min-height: 0; display: flex; align-items: stretch; \
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
                    {
                        let order: Vec<String> =
                            mixer.engines.iter().map(|e| e.name.clone()).collect();
                        rsx! {
                            for (i, engine) in mixer.engines.iter().enumerate() {
                                div {
                                    key: "{engine.name}",
                                    style: "flex: 0 0 auto;",
                                    EngineStrip {
                                        engine: engine.clone(),
                                        order: order.clone(),
                                        index: i,
                                    }
                                }
                            }
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
            KeyboardStrip { mixer: mixer.clone(), held, controllers }
        }
    }
}

/// Which node's Global Controls the band shows. Every level exposes the same
/// macro surface, so there is always one to show — with nothing selected it
/// is the **rig's**, driving every engine at once.
///
/// A module selection still shows its LAYER: anything deeper is the zoom's
/// job.
#[derive(Clone, PartialEq)]
enum MacroScope {
    /// The whole rig — every module under every engine.
    Rig,
    Engine(String),
    Layer { engine: String, layer: String },
}

impl MacroScope {
    fn of(selection: &Selection) -> Self {
        match selection {
            Selection::None => MacroScope::Rig,
            Selection::Engine(engine) => MacroScope::Engine(engine.clone()),
            Selection::Layer { engine, layer } | Selection::Module { engine, layer, .. } => {
                MacroScope::Layer { engine: engine.clone(), layer: layer.clone() }
            }
        }
    }

    /// What the band is called, and what level it is.
    fn heading(&self) -> (String, &'static str) {
        match self {
            MacroScope::Rig => ("All engines".to_string(), "rig controls"),
            MacroScope::Engine(engine) => (engine.clone(), "engine controls"),
            MacroScope::Layer { layer, .. } => (layer.clone(), "layer controls"),
        }
    }

    /// The colour of the thing being driven — the rig is nobody's engine, so
    /// it takes the master strip's neutral white.
    fn accent(&self) -> String {
        match self {
            MacroScope::Rig => "#e4e4e7".to_string(),
            MacroScope::Engine(engine) => engine_color(engine).to_string(),
            MacroScope::Layer { engine, .. } => engine_color(engine).to_string(),
        }
    }

    /// Whether this scope's knobs reach a given lane.
    fn reaches(&self, engine: &str, layer: &str) -> bool {
        match self {
            MacroScope::Rig => true,
            MacroScope::Engine(e) => e == engine,
            MacroScope::Layer { layer: l, .. } => l == layer,
        }
    }
}

/// **Every sound in the rig, as a curve** — its amp and filter envelopes and
/// its filter response, coloured by the engine it belongs to.
///
/// The band draws all of them at once, always: the ones the selection reaches
/// at full strength, the rest behind. That is the whole reason the shapes are
/// stacked rather than paged — you turn one knob and watch a family of curves
/// move together, and you can see what you are *not* moving.
/// Write a macro at whatever level is selected — the same three-way the macro
/// panel does, shared with the delay/reverb knobs beside the views.
fn set_scope_macro(rig: Option<KeysRigClient>, scope: MacroScope, id: String, v: f32) {
    spawn(async move {
        let Some(rig) = rig else { return };
        let _ = match scope {
            MacroScope::Rig => rig.set_rig_global(id, v).await,
            MacroScope::Engine(engine) => rig.set_engine_global(engine, id, v).await,
            MacroScope::Layer { layer, .. } => rig.set_layer_global(layer, id, v).await,
        };
    });
}

/// **Every lane's delay and reverb**, for the time-domain views. One entry per
/// loaded lane (not per module): a lane's send is a lane-level decision, and
/// six engines of per-module tails would be unreadable. Module A speaks for
/// the lane, which is also the module a lane preset fills first.
fn rig_time_fx(mixer: &KeysMixer, scope: &MacroScope) -> Vec<crate::time_fx::FxLane> {
    let mut lanes = Vec::new();
    for engine in mixer.engines.iter() {
        let color = engine_color(&engine.name).to_string();
        for layer in engine.layers.iter() {
            let Some(m) = layer.modules.iter().find(|m| m.live) else { continue };
            lanes.push(crate::time_fx::FxLane {
                label: layer.name.clone(),
                color: color.clone(),
                focus: scope.reaches(&engine.name, &layer.name),
                delay_ms: m.dly_time_ms,
                div: m.dly_div,
                feedback: m.dly_feedback,
                delay_mix: m.dly_mix,
                decay: m.amb_decay,
                size: m.amb_size,
                verb_mix: m.amb_mix,
                predelay_ms: m.amb_predelay_ms,
            });
        }
    }
    lanes
}

fn rig_curves(mixer: &KeysMixer, scope: &MacroScope) -> Vec<ModuleCurve> {
    let mut curves = Vec::new();
    for engine in mixer.engines.iter() {
        let color = engine_color(&engine.name).to_string();
        for layer in engine.layers.iter() {
            let focus = scope.reaches(&engine.name, &layer.name);
            let many = layer.modules.iter().filter(|m| m.live).count() > 1;
            for m in layer.modules.iter().filter(|m| m.live) {
                curves.push(ModuleCurve {
                    // The legend names the lane, and the slot too when a lane
                    // has more than one sound in it.
                    slot: if many {
                        format!("{}·{}", layer.name, m.slot)
                    } else {
                        layer.name.clone()
                    },
                    color: color.clone(),
                    amp: Adsr {
                        attack_ms: m.amp_env.attack_ms,
                        decay_ms: m.amp_env.decay_ms,
                        sustain: m.amp_env.sustain,
                        release_ms: m.amp_env.release_ms,
                    },
                    filter: Adsr {
                        attack_ms: m.filter_env.attack_ms,
                        decay_ms: m.filter_env.decay_ms,
                        sustain: m.filter_env.sustain,
                        release_ms: m.filter_env.release_ms,
                    },
                    cutoff_hz: m.cutoff_hz,
                    resonance: m.resonance,
                    unison: m.unison,
                    detune: m.detune,
                    vib_rate: m.vib_rate,
                    vib_depth: m.vib_depth,
                    vib_delay_ms: m.vib_delay_ms,
                    dim: !m.enabled || layer.muted || engine.muted,
                    focus,
                });
            }
        }
    }
    curves
}

/// **The macro band** — Global Controls, always. Under the strips that aim
/// them.
///
/// It follows the selection rather than the zoom: picking a card or a lane is
/// already how the browser is aimed, so it is also how the knobs are. With
/// nothing selected it drives the whole rig — every engine runs the same
/// macro surface, so one Filter, one Envelope, one Ambience over all of them
/// is a real control, not a placeholder. Each level is an offset into the one
/// beneath once that level stops agreeing with itself: the panels say
/// "offset" and print the spread.
#[component]
fn MacroBand() -> Element {
    let rig = use_hook(try_consume_context::<KeysRigClient>);
    let state = use_hook(try_consume_context::<crate::state::KeysViewState>);
    let selection = crate::selection::use_selection();
    let mut macros = use_signal(Vec::<KeysMacro>::new);

    let scope = MacroScope::of(&selection.read());
    let accent = scope.accent();

    // Re-pull whenever the selection moves or the rig publishes a mixer —
    // every macro move republishes, so the band reflects what it just did.
    // Both are read INSIDE the effect: that is what makes it re-run, and what
    // keeps it from firing on a selection it captured once at mount.
    {
        let rig = rig.clone();
        use_effect(move || {
            // Depends on the SELECTION only. It used to track the mixer too,
            // so that a fader move refreshed the readouts — but a knob write
            // republishes the mixer, which made every drag tick a fetch and a
            // full re-render of the band, the curves and both time-FX views.
            // That was the freeze. The knob paints its own drag locally, so
            // nothing is waiting on this.
            let scope = MacroScope::of(&selection.read());
            let rig = rig.clone();
            spawn(async move {
                let Some(rig) = rig else { return };
                match scope {
                    MacroScope::Rig => {
                        if let Ok(m) = rig.rig_macros().await {
                            macros.set(m);
                        }
                    }
                    MacroScope::Engine(engine) => {
                        if let Ok(d) = rig.engine_detail(engine).await {
                            macros.set(d.macros);
                        }
                    }
                    MacroScope::Layer { layer, .. } => {
                        if let Ok(d) = rig.layer_detail(layer, 0).await {
                            macros.set(d.layer_macros);
                        }
                    }
                }
            });
        });
    }

    let items = macros.read().clone();
    // Every sound in the rig, drawn behind the knobs that move it. Read from
    // the live mixer, so the curves follow a fader, a mute or a patch load
    // without the band asking for anything.
    let curves = state
        .map(|s| rig_curves(&s.mixer.read(), &scope))
        .unwrap_or_default();
    let fx_lanes = state.map(|s| rig_time_fx(&s.mixer.read(), &scope)).unwrap_or_default();
    // The time-domain groups get their own knobs beside their pictures; the
    // macro panel below still holds everything else.
    let dly_macros: Vec<KeysMacro> =
        items.iter().filter(|m| m.group == "Delay").cloned().collect();
    let amb_macros: Vec<KeysMacro> =
        items.iter().filter(|m| m.group == "Ambience").cloned().collect();

    rsx! {
        div {
            // The band owns the room between the engine strips and the
            // keyboard: the strips size to their cards, so everything left
            // over is the band's, which is where the delay and reverb
            // visualizations go. It scrolls vertically when it outgrows that
            // room, never sideways — everything the level has fits the width.
            style: "flex: 1; min-height: 0; overflow-y: auto; overflow-x: hidden; \
                    display: flex; flex-direction: column; gap: 8px; padding: 10px 12px; \
                    border-top: 1px solid #1c1c1f; background: #0a0a0c;",
            // The time-domain picture, above the knobs that shape it: one
            // delay and one reverb, every loaded lane drawn on each in its
            // engine's colour, the selected scope's lanes solid.
            div {
                style: "display: flex; flex-wrap: wrap; gap: 18px; padding: 4px 2px 2px; \
                        align-items: flex-start;",
                crate::time_fx::DelayView {
                    lanes: fx_lanes.clone(),
                    macros: dly_macros,
                    accent: "#38bdf8".to_string(),
                    on_change: {
                        let (rig, scope) = (rig.clone(), scope.clone());
                        move |(id, v): (String, f32)| set_scope_macro(rig.clone(), scope.clone(), id, v)
                    },
                }
                crate::time_fx::ReverbView {
                    lanes: fx_lanes,
                    macros: amb_macros,
                    accent: "#a78bfa".to_string(),
                    on_change: {
                        let (rig, scope) = (rig.clone(), scope.clone());
                        move |(id, v): (String, f32)| set_scope_macro(rig.clone(), scope.clone(), id, v)
                    },
                }
            }
            crate::macro_panel::MacroPanel {
                macros: items,
                curves,
                accent: accent.clone(),
                on_change: {
                    let (rig, scope) = (rig.clone(), scope.clone());
                    move |(id, v): (String, f32)| {
                        let (rig, scope) = (rig.clone(), scope.clone());
                        spawn(async move {
                            let Some(rig) = rig else { return };
                            let _ = match scope {
                                MacroScope::Rig => rig.set_rig_global(id, v).await,
                                MacroScope::Engine(engine) => {
                                    rig.set_engine_global(engine, id, v).await
                                }
                                MacroScope::Layer { layer, .. } => {
                                    rig.set_layer_global(layer, id, v).await
                                }
                            };
                        });
                    }
                },
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
fn KeyboardStrip(
    mixer: KeysMixer,
    held: Vec<u8>,
    #[props(default)] controllers: crate::controllers::Controllers,
) -> Element {
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
            div { style: "display: flex; align-items: flex-end; gap: 14px; min-width: 0;",
            // Pedals and wheels, laid out as the hardware is: feet at the far
            // left, then the left hand's wheels, then the keys.
            crate::controllers::ControllerStrip { controllers, height_px: 82 }
            div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 6px;",
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
fn EngineStrip(
    engine: KeysEngineModel,
    /// Every engine name in mixer order, and this one's place in it — a card
    /// can only be moved relative to its siblings.
    #[props(default)]
    order: Vec<String>,
    #[props(default)]
    index: usize,
) -> Element {
    let rig = use_hook(try_consume_context::<KeysRigClient>);
    let mut zoom = crate::zoom::use_zoom();
    let mut selection = crate::selection::use_selection();
    // What the engine is actually putting out — its card's bottom edge.
    let peak = peak_of("engine", &engine.name);
    let picked = *selection.read() == Selection::Engine(engine.name.clone());
    let pick_name = engine.name.clone();
    let accent = engine_color(&engine.name);
    let muted = engine.muted;
    let name = engine.name.clone();
    let open_name = engine.name.clone();
    let dbl_name = engine.name.clone();
    let first = index == 0;
    let last = order.is_empty() || index + 1 >= order.len();
    // Moving a card is a swap with its neighbour, sent as the whole order.
    let reorder = {
        let rig = rig.clone();
        let order = order.clone();
        move |to: usize| {
            let (rig, mut order) = (rig.clone(), order.clone());
            if to >= order.len() {
                return;
            }
            order.swap(index, to);
            spawn(async move {
                if let Some(r) = rig { let _ = r.set_engine_order(order).await; }
            });
        }
    };

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
            onclick: move |e: MouseEvent| {
                e.stop_propagation();
                selection.set(Selection::Engine(pick_name.clone()));
            },
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
            // …and the level it is REACHING is the card's bottom edge: the
            // fader you set down one side, what came out along the base.
            EdgeMeter {
                peak,
                accent: accent.to_string(),
                dimmed: muted,
                left_px: 9,
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
                    {
                        let back = reorder.clone();
                        let fwd = reorder.clone();
                        rsx! {
                            button {
                                style: move_style(first),
                                title: "Move left",
                                disabled: first,
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    back(index.saturating_sub(1));
                                },
                                "‹"
                            }
                            button {
                                style: move_style(last),
                                title: "Move right",
                                disabled: last,
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    fwd(index + 1);
                                },
                                "›"
                            }
                        }
                    }
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
            // Whatever this engine embeds in its card — the Drone's key
            // selector today; other engines can claim the slot later. It
            // extends the card sideways: the strips set the card's height and
            // nothing embedded is allowed to push it taller.
            if let Some(drone) = engine.drone.clone() {
                div {
                    style: "flex: 0 0 auto; display: flex; align-items: stretch; \
                            margin-left: 12px; padding-left: 12px; \
                            border-left: 1px solid #1c1c21;",
                    DroneKeys { engine: engine.name.clone(), drone }
                }
            }
        }
    }
}


/// **A drone engine's controls** — the card-embedded panel that replaces
/// playing it.
///
/// A drone is chosen, not performed: a key, the octave it sits in, and
/// whether it is holding. Dropdowns rather than a grid of twelve buttons —
/// the card has one narrow column to spend, and a picker leaves room for the
/// parameters a pad player actually reaches for as they land.
#[component]
fn DroneKeys(engine: String, drone: signal_keys_proto::KeysDrone) -> Element {
    const KEYS: [&str; 12] =
        ["C", "C♯", "D", "E♭", "E", "F", "F♯", "G", "A♭", "A", "B♭", "B"];
    let rig = use_hook(try_consume_context::<KeysRigClient>);
    let accent = engine_color(&engine).to_string();
    let playing = drone.playing;
    let key = drone.key.min(11) as usize;
    let octave = drone.octave.clamp(1, 5);

    // Every change is the whole state: the drone has three fields and sending
    // them together keeps the card and the engine from disagreeing.
    let send = {
        let (rig, engine) = (rig.clone(), engine.clone());
        move |(k, o, p): (u32, i32, bool)| {
            let (rig, engine) = (rig.clone(), engine.clone());
            spawn(async move {
                if let Some(r) = rig {
                    let _ = r.set_drone(engine, k, o, p).await;
                }
            });
        }
    };

    rsx! {
        div {
            style: "display: flex; flex-direction: column; gap: 8px; width: 124px;",
            {
                let send_key = send.clone();
                let send_oct = send.clone();
                rsx! {
                    DroneField {
                        label: "Key".to_string(),
                        current: KEYS[key].to_string(),
                        options: KEYS.iter().map(|k| k.to_string()).collect::<Vec<_>>(),
                        selected: key,
                        accent: accent.clone(),
                        on_pick: move |i: usize| send_key((i as u32, octave, playing)),
                    }
                    DroneField {
                        label: "Octave".to_string(),
                        current: format!("{octave}"),
                        options: (1..=5).map(|o| o.to_string()).collect::<Vec<_>>(),
                        selected: (octave - 1).max(0) as usize,
                        accent: accent.clone(),
                        on_pick: move |i: usize| send_oct((key as u32, i as i32 + 1, playing)),
                    }
                }
            }
            // Hold: the drone's only verb.
            {
                let send_hold = send.clone();
                rsx! {
                    button {
                        style: format!(
                            "appearance: none; border: 1px solid {}; border-radius: 8px; \
                             padding: 7px 0; cursor: pointer; font-size: 10px; font-weight: 800; \
                             letter-spacing: 0.08em; background: {}; color: {};",
                            if playing { accent.clone() } else { "#26262b".to_string() },
                            if playing { "#101216" } else { "#0f0f12" },
                            if playing { accent.clone() } else { "#52525b".to_string() },
                        ),
                        title: "The drone holds until you stop it — it takes no MIDI",
                        onclick: move |e: MouseEvent| {
                            e.stop_propagation();
                            send_hold((key as u32, octave, !playing));
                        },
                        if playing { "HOLDING" } else { "HOLD" }
                    }
                }
            }
        }
    }
}

/// One labelled dropdown in the drone panel.
#[component]
fn DroneField(
    label: String,
    current: String,
    options: Vec<String>,
    selected: usize,
    accent: String,
    on_pick: EventHandler<usize>,
) -> Element {
    let mut open = use_signal(|| false);

    rsx! {
        div { style: "display: flex; align-items: center; gap: 6px; position: relative;",
            span { style: "flex: 1; font-size: 9px; color: #52525b;", "{label}" }
            button {
                style: format!(
                    "appearance: none; min-width: 52px; border: 1px solid #26262b; \
                     border-radius: 6px; padding: 3px 8px; cursor: pointer; background: #101216; \
                     color: {accent}; font-size: 11px; font-weight: 700; text-align: left;",
                ),
                onclick: move |e: MouseEvent| {
                    e.stop_propagation();
                    open.toggle();
                },
                "{current}"
            }
            if open() {
                div {
                    style: "position: absolute; top: 100%; right: 0; z-index: 60; margin-top: 3px; \
                            max-height: 168px; overflow-y: auto; display: flex; \
                            flex-direction: column; gap: 1px; padding: 4px; min-width: 62px; \
                            border: 1px solid #2b2b31; border-radius: 8px; background: #0c0c0f; \
                            box-shadow: 0 12px 28px #000c;",
                    for (i, option) in options.iter().enumerate() {
                        {
                            let here = i == selected;
                            let accent = accent.clone();
                            rsx! {
                                button {
                                    key: "{i}",
                                    style: format!(
                                        "appearance: none; border: none; border-radius: 5px; \
                                         padding: 4px 8px; cursor: pointer; text-align: left; \
                                         font-size: 11px; font-weight: 600; background: {}; color: {};",
                                        if here { "#101821" } else { "transparent" },
                                        if here { accent } else { "#a1a1aa".to_string() },
                                    ),
                                    onclick: move |e: MouseEvent| {
                                        e.stop_propagation();
                                        open.set(false);
                                        on_pick.call(i);
                                    },
                                    "{option}"
                                }
                            }
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
    // The lane's own level, metered along the bottom of its letter.
    let peak = peak_of("layer", &layer.name);
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
            // hand lands without looking — and its bottom edge is the lane's
            // meter, so the switch tells you whether the lane is sounding.
            div { style: "position: relative; display: flex;",
            button {
                style: format!(
                    "appearance: none; flex: 1; border: 1px solid {}; border-radius: 8px; cursor: pointer; \
                     padding: 7px 0 9px; font-size: 14px; font-weight: 800; letter-spacing: 0.06em; \
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
                EdgeMeter {
                    peak,
                    radius_px: 8,
                    accent: accent.clone(),
                    dimmed: layer.muted || !layer.live,
                }
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

/// The engine card's move chevrons — quiet, and gone at the ends of the row.
fn move_style(at_end: bool) -> String {
    format!(
        "appearance: none; border: none; background: transparent; padding: 0 3px; \
         font-size: 12px; line-height: 1; color: {}; cursor: {};",
        if at_end { "#26262b" } else { "#52525b" },
        if at_end { "default" } else { "pointer" },
    )
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
