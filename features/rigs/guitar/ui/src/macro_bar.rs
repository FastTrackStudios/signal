//! The macro bar — a strip of macro knobs across the stage view, directly
//! above the footswitch grid in Profile and Setlist modes.
//!
//! A port of the legacy desktop app's macro bar
//! (`apps/desktop/src/signal_views/macro_bar.rs`: `MacroBar` / `MacroCell` /
//! `DropdownPanel` / `SubMacroDropdown` / `DualRowDropdown`), look and
//! behaviour kept: a label in the knob's colour with a ▾ when it has a
//! panel, a mini knob, a mono readout; hovering a cell drops its panel of
//! child knobs, bridged across the gap so the pointer can cross into it.
//!
//! What moved: the engine is the rig's (`signal_guitar::macros`) — the bar
//! draws [`MacroKnobView`]s and sends [`set_macro`](RigClient::set_macro),
//! so a footswitch or MIDI can turn the same knobs. And Blitz has no
//! Tailwind named groups for us (the app's sheet is compiled, and
//! `group-hover/macro` is not in it) and no `window.innerWidth`: hover is a
//! pair of `onmouseenter`/`onmouseleave` signals, the panel's entrance a
//! keyframe animation, and its nudge inside the window comes from where its
//! cell sits in the row.

use dioxus::prelude::*;

use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{MacroChildView, MacroKnobView, MacroResult, MacroSave, MacroTune, MacroTuneView};
use signal_widgets::arc::{angle_for_value, arc_path, arc_point, SENSITIVITY};
use signal_widgets::drag_bus::{DragBus, DragEvent};

use crate::param_writer::ParamWriter;

/// Hold a macro's panel open whatever the pointer does — a picture of the
/// hover state (`rig_shot`'s `RIG_SHOT_MACRO`). Provided by the host.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct MacroPanelOpen(pub Option<String>);

/// Open that held-open panel in tune mode (`rig_shot`'s `RIG_SHOT_TUNE`):
/// 1, or 2 with its first range knob's top handle lit as under the pointer.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct MacroTuneMode(pub u8);

/// A call's answer to show in a bar knob's panel header from the start
/// (`rig_shot`'s `RIG_SHOT_TUNE_SAVE`): `(bar knob, result)`.
#[derive(Clone, PartialEq, Debug)]
pub struct MacroShotStatus(pub Option<(String, MacroResult)>);

/// A knob label with no colour of its own (legacy `#94a3b8`).
const MUTED: &str = "#94a3b8";
/// The panel's entrance: the legacy `opacity-0 scale-95 translate-y-[-4px]`
/// → shown, 150 ms ease-out, as a keyframe (a panel mounts on hover, so
/// there is no earlier state to transition from).
const CSS: &str = "@keyframes macro-drop{from{opacity:0;transform:scale(0.95) translateY(-4px)}to{opacity:1;transform:scale(1) translateY(0)}}\
@keyframes macro-drop-up{from{opacity:0;transform:scale(0.95) translateY(4px)}to{opacity:1;transform:scale(1) translateY(0)}}";

/// The bar's wire: knob moves coalesced (a drag is an edit per pointer
/// event), pads sent straight, and the bar's state for optimistic moves.
#[derive(Clone)]
struct Wire {
    rig: Option<RigClient>,
    /// Knob moves, coalesced (a drag is an edit per pointer event).
    writer: ParamWriter,
    /// Tune-mode value edits, coalesced the same way: the "block" is
    /// `bar knob␟knob␟block␟param`, the "param" the op.
    tuner: ParamWriter,
    macros: Signal<Vec<MacroKnobView>>,
    /// The last thing a call on a bar knob's panel said, by bar knob —
    /// shown in its header.
    status: Signal<Option<(String, MacroResult)>>,
}

/// The separator in a tuner key.
const SEP: char = '\u{1f}';

/// A knob "value" meaning reset, on the knob-move queue (a knob's values
/// are 0..1).
const RESET: f32 = -1.0;

impl Wire {
    /// Move knob `id` — here at once, on the rig as fast as it answers.
    fn set(&self, id: &str, value: f32) {
        let mut macros = self.macros;
        macros.with_mut(|ks| {
            for k in ks.iter_mut() {
                if k.id == id {
                    k.value = value;
                }
                for c in k.children.iter_mut().filter(|c| c.id == id) {
                    c.value = value;
                }
            }
        });
        self.writer.set(id, "", value);
    }

    /// Show `r` in `knob`'s panel header — an error always, a success when
    /// it has something to say.
    fn show(&self, knob: &str, r: MacroResult) {
        if !r.ok || !r.message.is_empty() {
            let mut status = self.status;
            status.set(Some((knob.to_string(), r)));
        }
    }

    /// Call the rig and show what it says in `knob`'s header.
    fn run<F, Fut, E>(&self, knob: &str, f: F)
    where
        F: FnOnce(RigClient) -> Fut + 'static,
        Fut: std::future::Future<Output = Result<MacroResult, E>> + 'static,
        E: std::fmt::Debug + 'static,
    {
        let Some(r) = self.rig.clone() else {
            self.show(knob, MacroResult { ok: false, message: "Not connected to the rig".into(), offer: String::new() });
            return;
        };
        let (me, knob) = (self.clone(), knob.to_string());
        spawn(async move {
            let res = f(r).await.unwrap_or_else(|e| MacroResult {
                ok: false,
                message: format!("The rig did not answer: {e:?}"),
                offer: String::new(),
            });
            me.show(&knob, res);
        });
    }

    /// A tune-mode value edit on `t` (under bar knob `parent`): here at
    /// once, on the rig coalesced.
    fn tune(&self, parent: &str, t: &MacroTuneView, op: &str, value: f32) {
        let mut macros = self.macros;
        macros.with_mut(|ks| {
            for k in ks.iter_mut().filter(|k| k.id == parent) {
                k.tuned = true;
                for v in k.tune.iter_mut().filter(|v| v.knob == t.knob && v.block == t.block && v.param == t.param) {
                    match op {
                        "min" => {
                            v.lo = value;
                            v.min_set = true;
                        }
                        "max" => {
                            v.hi = value;
                            v.max_set = true;
                        }
                        "off" => v.off = value >= 0.5,
                        "enter" => v.enter = value,
                        _ => {}
                    }
                    v.edited = true;
                    v.source = "tuning".into();
                }
            }
        });
        let key = [parent, &t.knob, &t.block, &t.param].join(&SEP.to_string());
        self.tuner.set(key, op.to_string(), value);
    }

    /// A tune-mode edit that is not a value (a curve, a reset): straight
    /// to the rig, its answer in the header.
    fn tune_op(&self, parent: &str, t: &MacroTuneView, op: &str, text: &str) {
        let tune = MacroTune {
            knob: t.knob.clone(),
            block: t.block.clone(),
            param: t.param.clone(),
            op: op.to_string(),
            value: 0.0,
            text: text.to_string(),
        };
        self.run(parent, move |r| async move { r.tune_macro(tune).await });
    }

    fn pad(&self, id: &str, on: bool) {
        let mut macros = self.macros;
        macros.with_mut(|ks| {
            for c in ks.iter_mut().flat_map(|k| k.children.iter_mut()).filter(|c| c.id == id) {
                c.bypassed = !on;
            }
        });
        let (id, parent) = (id.to_string(), self.parent_of(id));
        self.run(&parent, move |r| async move { r.set_macro_pad(id, on).await });
    }

    /// Double-click in play: a bar knob to rest, a panel knob to where its
    /// bar knob puts it. Sent down the same queue as the knob's moves, so a
    /// move from the click itself cannot land after it.
    fn reset(&self, id: &str) {
        let mut macros = self.macros;
        macros.with_mut(|ks| {
            for k in ks.iter_mut().filter(|k| k.id == id) {
                k.value = k.rest;
            }
        });
        self.writer.set(id, "", RESET);
    }

    /// The bar knob `id` belongs to (itself, for a bar knob).
    fn parent_of(&self, id: &str) -> String {
        self.macros
            .peek()
            .iter()
            .find(|k| k.id == id || k.children.iter().any(|c| c.id == id))
            .map_or_else(|| id.to_string(), |k| k.id.clone())
    }
}

/// When a dual-row panel's Type link is on, the knob in the other row that
/// follows `id` — the legacy `linked_mirror`, less its Time link: the macros
/// never move timing, so the two delays' times stay as the patch has them.
#[must_use]
pub fn linked_mirror(id: &str, prefix: &str, type_linked: bool) -> Option<String> {
    if type_linked {
        let (t1, t2) = (format!("{prefix}-type1"), format!("{prefix}-type2"));
        if id == t1 {
            return Some(t2);
        }
        if id == t2 {
            return Some(t1);
        }
    }
    None
}

/// A child's readout, in the rig's own units.
#[must_use]
pub fn child_readout(c: &MacroChildView) -> String {
    if c.fmt.is_empty() {
        return format!("{:.0}%", c.value * 100.0);
    }
    fmt_value(&c.fmt, c.param, c.aux)
}

/// A param value in the rig's own units, by format (see
/// [`MacroChildView::fmt`]).
#[must_use]
pub fn fmt_value(fmt: &str, v: f32, aux: f32) -> String {
    let pick = |names: &[&str]| names.get(v.round().max(0.0) as usize).copied().unwrap_or("—").to_string();
    match fmt {
        "pan" if v.abs() < 0.005 => "C".to_string(),
        "pan" => format!("{}{:.0}", if v < 0.0 { "L" } else { "R" }, v.abs() * 100.0),
        "db" => crate::control::level_fmt(v),
        "db_gain" => format!("{v:+.1} dB"),
        "hz" => crate::control::cut_fmt(v),
        "ms" if v >= 1000.0 => format!("{:.2} s", v / 1000.0),
        "ms" if v < 10.0 => format!("{v:.1} ms"),
        "ms" => format!("{v:.0} ms"),
        "s" => format!("{v:.2} s"),
        "verb_s" => crate::control::decay_fmt_for(aux, 0.0)(v),
        "div" => pick(&crate::control::DIV_LABELS),
        "pct" => format!("{:.0}%", v * 100.0),
        "ratio" => format!("{v:.1}:1"),
        "delay_style" => pick(&crate::control::DELAY_ALGOS),
        "verb_algo" => pick(&crate::control::VERB_ALGOS),
        "semitones" => signed(v.round() as i32),
        "interval" => interval_label(v.round() as i32),
        _ => format!("{:.0}%", v * 100.0),
    }
}

/// `+12`, `−12`, `0` — a real minus, so the column of intervals lines up.
fn signed(n: i32) -> String {
    match n {
        0 => "0".to_string(),
        n if n > 0 => format!("+{n}"),
        n => format!("\u{2212}{}", -n),
    }
}

/// The Ice machine's interval menu (the TimeLine MX's): −12..−1, ±25/50
/// cents, +1..+12, +19, +24, then Free.
fn interval_label(i: i32) -> String {
    match i {
        0..=11 => signed(i - 12),
        12 => "\u{2212}50c".into(),
        13 => "\u{2212}25c".into(),
        14 => "+25c".into(),
        15 => "+50c".into(),
        16..=27 => signed(i - 15),
        28 => "+19".into(),
        29 => "+24".into(),
        _ => "Free".into(),
    }
}

// ============================================================================
// MacroBar — horizontal strip of macro knobs
// ============================================================================

#[component]
pub fn MacroBar(
    macros: Signal<Vec<MacroKnobView>>,
    /// Open the panels upward (no room below: the grid is short or gone).
    #[props(default)]
    drop_up: bool,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let forced = use_hook(|| try_consume_context::<MacroPanelOpen>().and_then(|o| o.0));
    let shot_status = use_hook(|| try_consume_context::<MacroShotStatus>().and_then(|s| s.0));
    let status = use_signal(move || shot_status);
    use_context_provider(|| {
        let rig = rig.clone();
        let send = rig.clone();
        let send_tune = rig.clone();
        Wire {
            rig,
            writer: ParamWriter::new(
                move |(id, _, value)| {
                    let r = send.clone();
                    Box::pin(async move {
                        if let Some(r) = r {
                            let _ = if value <= RESET { r.reset_macro(id).await } else { r.set_macro(id, value).await };
                        }
                    })
                },
                |task| {
                    spawn(task);
                },
            ),
            tuner: ParamWriter::new(
                move |(key, op, value)| {
                    let r = send_tune.clone();
                    let mut status = status;
                    Box::pin(async move {
                        let parts: Vec<&str> = key.split(SEP).collect();
                        let (Some(r), [parent, knob, block, param]) = (r, parts.as_slice()) else { return };
                        let tune = MacroTune {
                            knob: (*knob).to_string(),
                            block: (*block).to_string(),
                            param: (*param).to_string(),
                            op,
                            value,
                            text: String::new(),
                        };
                        let res = r.tune_macro(tune).await.unwrap_or_else(|e| MacroResult {
                            ok: false,
                            message: format!("The rig did not answer: {e:?}"),
                            offer: String::new(),
                        });
                        if !res.ok {
                            status.set(Some(((*parent).to_string(), res)));
                        }
                    })
                },
                |task| {
                    spawn(task);
                },
            ),
            macros,
            status,
        }
    });
    let knobs = macros.read().clone();
    if knobs.is_empty() {
        return rsx! {};
    }
    let count = knobs.len();
    let at = |id: &str| knobs.iter().position(|k| k.id == id);

    rsx! {
        style { {CSS} }
        div {
            // Above the grid it overhangs: a positioned layer of its own.
            style: "flex-shrink: 0; padding: 8px 12px; position: relative; z-index: 40; \
                    border-bottom: 1px solid rgba(39,39,42,0.5); background: rgba(9,9,11,0.3);",
            div { style: "display: flex; align-items: flex-start; width: 100%;",
                for (i, k) in knobs.iter().enumerate() {
                    MacroCell {
                        key: "{k.id}",
                        knob: k.clone(),
                        index: i,
                        count,
                        // Clarity hangs under the Delay knob.
                        anchor_cells: at(&k.anchor).map_or(0, |a| i.saturating_sub(a)),
                        drop_up,
                        forced: forced.as_deref() == Some(k.id.as_str()),
                    }
                }
            }
        }
    }
}

// ============================================================================
// MacroCell — one knob cell; hovering it drops its panel
// ============================================================================

#[component]
fn MacroCell(
    knob: MacroKnobView,
    index: usize,
    count: usize,
    anchor_cells: usize,
    drop_up: bool,
    forced: bool,
) -> Element {
    let wire = use_context::<Wire>();
    // The pointer is over the cell or its panel (`group-hover/macro`).
    let mut hovered = use_signal(|| false);
    // Over the knob cell itself (its own `hover:` background).
    let mut over_cell = use_signal(|| false);
    // A knob in the cell or its panel is being dragged: the panel stays
    // while the pointer wanders off it.
    let dragging = use_signal(|| false);
    // Tune mode (the panel's ✎): every param the knob moves, with its
    // range; the panel stays until it is put away.
    let shot_tune = use_hook(|| try_consume_context::<MacroTuneMode>().map_or(0, |t| t.0));
    let mut tuning = use_signal(move || forced && shot_tune > 0);
    // Leaving tune mode with unsaved edits: asked inline.
    let mut leaving = use_signal(|| false);
    // Where Save and Reset go: each block's block preset, or the module
    // snapshot that owns the block. Drive's stages are pedals, not block
    // presets: the module snapshot only.
    let module_only = knob.id == "drive";
    let mut scope = use_signal(move || if module_only { "module" } else { "block" });
    let host = signal_widgets::PopupHost::try_use();
    let has_children = !knob.children.is_empty();
    let has_panel = has_children || !knob.tune.is_empty();
    let id = knob.id.clone();
    // What the last call on this panel said (shown in the header; it keeps
    // the panel open while it shows).
    let mut status = wire.status;
    let mine = status.read().as_ref().filter(|(k, _)| *k == id).map(|(_, r)| r.clone());
    // A message clears itself after a few seconds; an offer waits for an
    // answer.
    {
        let id = id.clone();
        use_effect(move || {
            let current = status.read().clone();
            if let Some((k, r)) = current {
                if k == id && r.offer.is_empty() {
                    spawn(async move {
                        architect::platform::sleep(std::time::Duration::from_secs(5)).await;
                        if status.peek().as_ref().is_some_and(|(k2, r2)| *k2 == k && *r2 == r) {
                            status.set(None);
                        }
                    });
                }
            }
        });
    }
    let open = forced || hovered() || dragging() || tuning() || mine.is_some();
    let color = if knob.color.is_empty() { MUTED.to_string() } else { knob.color.clone() };
    let at_rest = (knob.value - knob.rest).abs() < 0.005;
    let menu_items = {
        let mut snap = crate::kit::MenuItem::run("positions_snapshot", "Save positions to preset snapshot");
        if knob.snapshot.is_empty() {
            snap.disabled = Some("This patch plays no preset snapshot — its positions stay with the patch".into());
        } else {
            snap.label = format!("Save positions to {}", knob.snapshot);
        }
        vec![
            crate::kit::MenuItem::run("tune", format!("Tune {}…", knob.label)),
            crate::kit::MenuItem::run("positions_patch", "Save positions to this patch"),
            snap,
        ]
    };

    rsx! {
        div {
            style: "position: relative; flex: 1 1 0%; min-width: 0;",
            onmouseenter: move |_| hovered.set(true),
            onmouseleave: move |_| hovered.set(false),

            // Main knob cell
            div {
                style: format!(
                    "display: flex; flex-direction: column; align-items: center; gap: 2px; \
                     padding: 6px 0; border-radius: 12px; cursor: pointer; \
                     border: 1px solid transparent; background: {};",
                    if over_cell() { "rgba(39,39,42,0.4)" } else { "transparent" },
                ),
                onmouseenter: move |_| over_cell.set(true),
                onmouseleave: move |_| over_cell.set(false),
                // Right-click: tune it, or keep the bar's positions.
                oncontextmenu: {
                    let wire = wire.clone();
                    let id = id.clone();
                    let items = menu_items.clone();
                    move |e: MouseEvent| {
                        e.prevent_default();
                        e.stop_propagation();
                        let (wire, id) = (wire.clone(), id.clone());
                        crate::kit::context_menu(
                            host,
                            &e,
                            items.clone(),
                            EventHandler::new(move |p: crate::kit::Picked| match p.id {
                                "tune" => tuning.set(true),
                                "positions_patch" => {
                                    wire.run(&id, |r| async move { r.save_macro_positions("patch".into()).await });
                                }
                                "positions_snapshot" => {
                                    wire.run(&id, |r| async move { r.save_macro_positions("snapshot".into()).await });
                                }
                                _ => {}
                            }),
                        );
                    }
                },

                // Label (above knob)
                div { style: "display: flex; align-items: center; justify-content: center; gap: 2px; width: 100%;",
                    span {
                        style: "font-size: 10px; font-weight: 500; max-width: 56px; overflow: hidden; \
                                white-space: nowrap; color: {color};",
                        "{knob.label}"
                    }
                    if has_panel {
                        span {
                            style: format!("font-size: 8px; color: {};", if open { "#a1a1aa" } else { "#52525b" }),
                            "\u{25BE}"
                        }
                    }
                }

                MiniKnob {
                    value: knob.value,
                    color: color.clone(),
                    bipolar: knob.bipolar,
                    spread: knob.style == "spread",
                    rest: knob.rest,
                    dragging,
                    on_change: {
                        let wire = wire.clone();
                        let id = id.clone();
                        move |v: f32| wire.set(&id, v)
                    },
                    on_reset: {
                        let wire = wire.clone();
                        let id = id.clone();
                        move |()| wire.reset(&id)
                    },
                }

                // Value readout: grey at rest, bright once moved off it.
                span {
                    style: format!(
                        "font-size: 9px; font-family: ui-monospace, monospace; \
                         font-variant-numeric: tabular-nums; white-space: nowrap; color: {};",
                        if at_rest { "#a1a1aa" } else { "#e4e4e7" },
                    ),
                    "{knob.readout}"
                }
            }

            if has_panel && open {
                // Tune mode is taller: it rises over the page.
                DropdownPanel { index, count, anchor_cells, drop_up: drop_up || tuning(), still: forced,
                    PanelHeader {
                        label: knob.label.clone(),
                        tuning: tuning(),
                        module_only,
                        scope: scope().to_string(),
                        unsaved: knob.tuned,
                        leaving: leaving(),
                        status: mine.clone(),
                        on_tune: move |()| tuning.set(true),
                        on_scope: move |s: String| scope.set(if s == "module" { "module" } else { "block" }),
                        on_save: {
                            let wire = wire.clone();
                            let id = id.clone();
                            move |name: String| {
                                let save = MacroSave { knob: id.clone(), scope: scope().to_string(), name };
                                wire.run(&id, move |r| async move { r.save_macro_tune(save).await });
                                leaving.set(false);
                            }
                        },
                        on_reset: {
                            let wire = wire.clone();
                            let id = id.clone();
                            move |()| {
                                let (k, sc) = (id.clone(), scope().to_string());
                                wire.run(&id, move |r| async move { r.reset_macro_scope(k, sc).await });
                            }
                        },
                        on_leave: move |()| {
                            if knob.tuned {
                                leaving.set(true);
                            } else {
                                tuning.set(false);
                            }
                        },
                        on_discard: {
                            let wire = wire.clone();
                            let id = id.clone();
                            move |()| {
                                let k = id.clone();
                                wire.run(&id, move |r| async move { r.discard_macro_tune(k).await });
                                leaving.set(false);
                                tuning.set(false);
                            }
                        },
                        on_stay: move |()| leaving.set(false),
                        on_dismiss: move |()| status.set(None),
                    }
                    if tuning() {
                        TuneGrid {
                            parent: knob.id.clone(),
                            tune: knob.tune.clone(),
                            dragging,
                            highlight: forced && shot_tune >= 2,
                        }
                    } else if has_children {
                        match knob.layout.as_str() {
                            "dual" => rsx! {
                                DualRowDropdown {
                                    prefix: knob.id.clone(),
                                    headers: knob.headers.clone(),
                                    children_knobs: knob.children.clone(),
                                    dragging,
                                }
                            },
                            "grouped" => rsx! {
                                GroupedDropdown { children_knobs: knob.children.clone(), dragging }
                            },
                            _ => rsx! {
                                SubMacroDropdown { children_knobs: knob.children.clone(), dragging }
                            },
                        }
                    } else {
                        TargetList { tune: knob.tune.clone() }
                    }
                }
            }
        }
    }
}

// ============================================================================
// DropdownPanel — the panel under a cell, kept inside the window
// ============================================================================

/// The panel and its hover bridge. The legacy panel measured itself against
/// `window.innerWidth` (a webview eval) and nudged; Blitz has neither, so
/// the panel hangs from the side of its cell that keeps it in: the left
/// edge in the first third of the row, the right edge in the last third,
/// centred between — or, for a panel that belongs to another cell (Clarity,
/// under Delay), from that cell's left edge.
#[component]
fn DropdownPanel(
    index: usize,
    count: usize,
    anchor_cells: usize,
    drop_up: bool,
    /// Held open by the host: shown without the entrance (a picture is
    /// taken at time zero, where the entrance has not begun).
    still: bool,
    children: Element,
) -> Element {
    let place = if anchor_cells > 0 {
        // From the anchor cell's left edge, and no further right than the
        // end of the row.
        format!(
            "left: -{}%; max-width: {}%; overflow: hidden;",
            anchor_cells * 100,
            (count - index + anchor_cells) * 100,
        )
    } else if index * 3 < count {
        "left: 0;".to_string()
    } else if index * 3 >= count * 2 {
        "right: 0;".to_string()
    } else {
        "left: 50%; transform: translateX(-50%);".to_string()
    };
    let (edge, gap, anim, origin) = if drop_up {
        ("bottom: 100%;", "margin-bottom: 8px;", "macro-drop-up", "bottom center")
    } else {
        ("top: 100%;", "margin-top: 8px;", "macro-drop", "top center")
    };
    let anim = if still { "none".to_string() } else { format!("{anim} 150ms ease-out") };
    rsx! {
        // Invisible bridge: fills the gap between the cell and the panel.
        div { style: "position: absolute; {edge} left: 0; width: 100%; height: 8px;" }
        // The panel
        div { style: "position: absolute; {edge} {place} {gap} z-index: 50;",
            div {
                style: "border-radius: 12px; border: 1px solid rgba(63,63,70,0.8); \
                        background: rgba(24,24,27,0.95); padding: 8px; \
                        box-shadow: 0 20px 25px -5px rgba(0,0,0,0.5), 0 8px 10px -6px rgba(0,0,0,0.5); \
                        animation: {anim}; transform-origin: {origin};",
                {children}
            }
        }
    }
}

// ============================================================================
// SubMacroDropdown — a row of child knobs, with bypass pads
// ============================================================================

#[component]
fn SubMacroDropdown(children_knobs: Vec<MacroChildView>, dragging: Signal<bool>) -> Element {
    rsx! {
        div { style: "display: flex; align-items: flex-end; gap: 4px;",
            for child in children_knobs.iter() {
                ChildCell { key: "{child.id}", child: child.clone(), dragging, width: 68 }
            }
        }
    }
}

/// One knob of a panel: an ON/OFF pad when it has one, the label, the
/// knob, the readout.
#[component]
fn ChildCell(
    child: MacroChildView,
    dragging: Signal<bool>,
    /// Fixed width (`w-[68px]`), or 0 to fill a grid column.
    width: u32,
    /// Dual-row links: the knob in the other row that follows this one.
    #[props(default)]
    mirror: Option<String>,
) -> Element {
    let wire = use_context::<Wire>();
    let mut over = use_signal(|| false);
    let color = if child.color.is_empty() { MUTED.to_string() } else { child.color.clone() };
    // An empty drive slot plays nothing: dimmed, and nothing to turn.
    let dim = child.empty || (child.has_pad && child.bypassed);
    let at_rest = child.steps > 0 || (child.value - child.rest).abs() < 0.005;
    let readout = child_readout(&child);
    let id = child.id.clone();
    let width_css = if width > 0 { format!("width: {width}px;") } else { String::new() };
    let label_ink = if child.empty { "#71717a".to_string() } else { color.clone() };

    rsx! {
        div {
            title: "{child.tooltip}",
            style: format!(
                "{width_css} display: flex; flex-direction: column; align-items: center; gap: 4px; \
                 padding: 6px 0; border-radius: 8px; cursor: pointer; \
                 border: 1px solid transparent; background: {}; opacity: {};",
                if over() { "rgba(63,63,70,0.4)" } else { "transparent" },
                if dim { "0.4" } else { "1" },
            ),
            onmouseenter: move |_| over.set(true),
            onmouseleave: move |_| over.set(false),

            // Bypass pad
            if child.has_pad {
                {
                    let wire = wire.clone();
                    let id = id.clone();
                    let on = child.bypassed;
                    let empty = child.empty;
                    rsx! {
                        div {
                            style: if child.bypassed || empty {
                                "width: 48px; height: 16px; border-radius: 6px; font-size: 8px; font-weight: 700; \
                                 display: flex; align-items: center; justify-content: center; \
                                 border: 1px solid #52525b; background: rgba(39,39,42,0.6); color: #52525b; \
                                 text-transform: uppercase; letter-spacing: 0.05em;".to_string()
                            } else {
                                format!(
                                    "width: 48px; height: 16px; border-radius: 6px; font-size: 8px; font-weight: 700; \
                                     display: flex; align-items: center; justify-content: center; \
                                     border: 1px solid transparent; background: {color}; color: #18181b; \
                                     text-transform: uppercase; letter-spacing: 0.05em;"
                                )
                            },
                            onclick: move |e: MouseEvent| {
                                e.stop_propagation();
                                if !empty {
                                    wire.pad(&id, on);
                                }
                            },
                            if child.bypassed || empty { "OFF" } else { "ON" }
                        }
                    }
                }
            }

            // A drive stage: its slot, small, over the pedal it plays.
            if !child.slot.is_empty() {
                span {
                    style: "font-size: 7px; font-weight: 600; color: #71717a; text-transform: uppercase; \
                            letter-spacing: 0.06em; white-space: nowrap;",
                    "{child.slot}"
                }
            }
            // Label
            span {
                style: "font-size: 9px; font-weight: 500; max-width: 62px; overflow: hidden; \
                        white-space: nowrap; color: {label_ink};",
                "{child.label}"
            }
            if !child.subtitle.is_empty() {
                span {
                    style: "font-size: 7px; color: #a1a1aa; max-width: 62px; overflow: hidden; white-space: nowrap;",
                    "{child.subtitle}"
                }
            }

            MiniKnob {
                value: child.value,
                color: color.clone(),
                spread: false,
                rest: child.rest,
                dragging,
                disabled: child.empty,
                on_change: {
                    let wire = wire.clone();
                    let id = id.clone();
                    move |v: f32| {
                        wire.set(&id, v);
                        if let Some(m) = mirror.as_deref() {
                            wire.set(m, v);
                        }
                    }
                },
                on_reset: {
                    let wire = wire.clone();
                    let id = id.clone();
                    move |()| wire.reset(&id)
                },
            }

            // Value readout
            span {
                style: format!(
                    "font-size: 8px; font-family: ui-monospace, monospace; \
                     font-variant-numeric: tabular-nums; white-space: nowrap; color: {};",
                    if at_rest { "#71717a" } else { "#d4d4d8" },
                ),
                "{readout}"
            }
        }
    }
}

// ============================================================================
// Tune mode — the header, the target list, the range editors
// ============================================================================

/// A value in a tune view's units.
fn tune_readout(t: &MacroTuneView, v: f32) -> String {
    fmt_value(&t.fmt, v, t.aux)
}

/// The panel's header: its name hard left, the actions hard right — the
/// ✎ in play; tuning, where Save and Reset go, Reset, Save and put away,
/// with a dot while something is unsaved. Under it, what the last call
/// said, an offer to make a new block preset or module snapshot, or the
/// question on leaving with unsaved edits.
#[component]
fn PanelHeader(
    label: String,
    tuning: bool,
    scope: String,
    #[props(default)] module_only: bool,
    /// Something here is tuned and not saved.
    unsaved: bool,
    /// Put away was pressed with unsaved edits.
    leaving: bool,
    status: Option<MacroResult>,
    on_tune: EventHandler<()>,
    on_scope: EventHandler<String>,
    /// Save — with a name, into a new block preset or module snapshot.
    on_save: EventHandler<String>,
    on_reset: EventHandler<()>,
    on_leave: EventHandler<()>,
    on_discard: EventHandler<()>,
    on_stay: EventHandler<()>,
    on_dismiss: EventHandler<()>,
) -> Element {
    let seg = |on: bool| {
        format!(
            "padding: 2px 7px; font-size: 9px; font-weight: 600; cursor: pointer; border-radius: 4px; \
             white-space: nowrap; color: {}; background: {};",
            if on { "#18181b" } else { "#a1a1aa" },
            if on { "#e4e4e7" } else { "transparent" },
        )
    };
    let button = |primary: bool| {
        format!(
            "padding: 2px 8px; font-size: 9px; font-weight: 700; border-radius: 4px; cursor: pointer; \
             white-space: nowrap; color: {}; background: {};",
            if primary { "#18181b" } else { "#d4d4d8" },
            if primary { "#22d3ee" } else { "rgba(63,63,70,0.6)" },
        )
    };
    let where_ = if scope == "module" { "module snapshot" } else { "block preset" };
    let offer = status.as_ref().map(|s| s.offer.clone()).filter(|o| !o.is_empty());
    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 4px; padding: 0 2px 6px; min-width: 0;",
            div { style: "display: flex; align-items: center; gap: 6px; min-width: 0;",
                span {
                    style: "font-size: 9px; font-weight: 600; color: #71717a; text-transform: uppercase; \
                            letter-spacing: 0.05em; white-space: nowrap;",
                    if tuning { "Tune {label}" } else { "{label}" }
                }
                if unsaved {
                    span { title: "Tuned, not saved", style: "width: 6px; height: 6px; border-radius: 3px; background: #fbbf24; flex-shrink: 0;" }
                }
                div { style: "flex: 1 1 0%;" }
                if tuning {
                    if module_only {
                        span { style: "font-size: 9px; color: #a1a1aa; white-space: nowrap;", "Module snapshot" }
                    } else {
                        div {
                            title: "Where Save and Reset go",
                            style: "display: flex; gap: 1px; padding: 1px; border-radius: 5px; border: 1px solid #3f3f46;",
                            div { style: seg(scope != "module"), onclick: move |_| on_scope.call("block".into()), "Block preset" }
                            div { style: seg(scope == "module"), onclick: move |_| on_scope.call("module".into()), "Module snapshot" }
                        }
                    }
                    div {
                        title: "Clear what the {where_} says about these params — the next layer down plays",
                        style: button(false),
                        onclick: move |_| on_reset.call(()),
                        "Reset"
                    }
                    div {
                        title: if unsaved { format!("Keep these ranges in the {where_}") } else { "Nothing tuned yet".to_string() },
                        style: button(unsaved),
                        onclick: move |_| on_save.call(String::new()),
                        "Save"
                    }
                    div {
                        title: "Put away",
                        style: "padding: 0 4px; color: #a1a1aa; cursor: pointer; display: flex;",
                        onclick: move |_| on_leave.call(()),
                        fts_chrome::Glyph { icon: fts_chrome::Icon::Close, size: 11 }
                    }
                } else {
                    div {
                        title: "Tune how this macro moves each param",
                        style: "padding: 0 4px; color: #71717a; cursor: pointer; display: flex;",
                        onclick: move |_| on_tune.call(()),
                        fts_chrome::Glyph { icon: fts_chrome::Icon::Pencil, size: 11 }
                    }
                }
            }
            if leaving {
                div { style: "display: flex; align-items: center; gap: 6px;",
                    span { style: "font-size: 10px; color: #fbbf24; white-space: nowrap;", "Unsaved ranges —" }
                    div { style: button(true), onclick: move |_| on_save.call(String::new()), "Save" }
                    div { style: button(false), onclick: move |_| on_discard.call(()), "Discard" }
                    div { style: button(false), onclick: move |_| on_stay.call(()), "Keep tuning" }
                }
            }
            if let Some(s) = status.clone() {
                div { style: "display: flex; align-items: center; gap: 6px; min-width: 0;",
                    span {
                        style: format!(
                            "font-size: 10px; white-space: nowrap; overflow: hidden; color: {};",
                            if s.ok { "#4ade80" } else if offer.is_some() { "#fbbf24" } else { "#f87171" },
                        ),
                        "{s.message}"
                    }
                    if offer.is_none() {
                        div {
                            style: "padding: 0 2px; color: #71717a; cursor: pointer; display: flex;",
                            onclick: move |_| on_dismiss.call(()),
                            fts_chrome::Glyph { icon: fts_chrome::Icon::Close, size: 9 }
                        }
                    }
                }
            }
            if let Some(o) = offer {
                crate::kit::NamePrompt {
                    label: if o == "new_module_snapshot" { "Save as new module snapshot" } else { "Save as new block preset" },
                    initial: format!("{label} Tuned"),
                    placeholder: "Name",
                    on_done: move |name: Option<String>| match name {
                        Some(n) => on_save.call(n),
                        None => on_dismiss.call(()),
                    },
                }
            }
        }
    }
}

/// A single knob's panel in play: every param it moves, by block, with its
/// value now — names hard left, values hard right.
#[component]
fn TargetList(tune: Vec<MacroTuneView>) -> Element {
    let groups = group_tune(&tune);
    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 6px; width: 220px;",
            for (group, rows) in groups {
                div { key: "{group}", style: "display: flex; flex-direction: column; gap: 1px;",
                    span {
                        style: "font-size: 9px; font-weight: 600; color: #71717a; text-transform: uppercase; letter-spacing: 0.05em;",
                        "{group}"
                    }
                    for t in rows {
                        div {
                            key: "{t.knob}{t.param}",
                            style: "display: flex; align-items: center; gap: 8px; padding: 1px 2px;",
                            span { style: "font-size: 10px; color: {t.color}; white-space: nowrap;", "{t.label}" }
                            div { style: "flex: 1 1 0%;" }
                            span {
                                style: format!(
                                    "font-size: 10px; font-family: ui-monospace, monospace; white-space: nowrap; color: {};",
                                    if t.off { "#52525b" } else { "#d4d4d8" },
                                ),
                                if t.off { "off" } else { "{tune_readout(&t, t.live)}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Tune views by block, in order.
fn group_tune(tune: &[MacroTuneView]) -> Vec<(String, Vec<MacroTuneView>)> {
    let mut groups: Vec<(String, Vec<MacroTuneView>)> = Vec::new();
    for t in tune {
        match groups.iter_mut().find(|(g, _)| *g == t.group) {
            Some((_, r)) => r.push(t.clone()),
            None => groups.push((t.group.clone(), vec![t.clone()])),
        }
    }
    groups
}

/// Tune mode's body: a range editor for every param the bar knob and its
/// panel move, by block — two blocks to a column, so a wide knob (Space,
/// Width) stays short.
#[component]
fn TuneGrid(parent: String, tune: Vec<MacroTuneView>, dragging: Signal<bool>, highlight: bool) -> Element {
    let groups = group_tune(&tune);
    let rows = groups.len().min(2);
    rsx! {
        div {
            style: "display: grid; grid-template-rows: repeat({rows}, auto); grid-auto-flow: column; \
                    grid-auto-columns: max-content; column-gap: 14px; row-gap: 6px; align-items: start;",
            for (gi, (group, row)) in groups.into_iter().enumerate() {
                div { key: "{group}", style: "display: flex; flex-direction: column; gap: 2px;",
                    span {
                        style: "font-size: 9px; font-weight: 600; color: #71717a; text-transform: uppercase; \
                                letter-spacing: 0.05em; white-space: nowrap; padding: 0 2px;",
                        "{group}"
                    }
                    div { style: "display: flex; align-items: flex-end; gap: 4px;",
                        for (i, t) in row.into_iter().enumerate() {
                            TuneEditor {
                                key: "{t.knob}{t.block}{t.param}",
                                parent: parent.clone(),
                                t,
                                dragging,
                                highlight: highlight && gi == 0 && i == 0,
                            }
                        }
                    }
                }
            }
        }
    }
}

/// One param's range editor: its name, the range knob, `lo–hi`, and its
/// chips — the curve, Off, and a drive stage's entry.
#[component]
fn TuneEditor(parent: String, t: MacroTuneView, dragging: Signal<bool>, highlight: bool) -> Element {
    let color = if t.color.is_empty() { MUTED.to_string() } else { t.color.clone() };
    let range = if t.off {
        "off".to_string()
    } else {
        format!("{}–{}", tune_readout(&t, t.lo), tune_readout(&t, t.hi))
    };
    rsx! {
        div {
            style: "width: 64px; display: flex; flex-direction: column; align-items: center; gap: 3px; padding: 4px 0;",
            span {
                title: "{t.group} · {t.param}",
                style: format!(
                    "font-size: 9px; font-weight: 500; max-width: 62px; overflow: hidden; white-space: nowrap; color: {};",
                    if t.off { "#71717a" } else { color.as_str() },
                ),
                "{t.label}"
            }
            TuneKnob { parent: parent.clone(), t: t.clone(), color: color.clone(), dragging, highlight }
            span {
                style: format!(
                    "font-size: 8px; font-family: ui-monospace, monospace; white-space: nowrap; color: {};",
                    if t.off { "#52525b" } else { "#d4d4d8" },
                ),
                "{range}"
            }
            div { style: "display: flex; gap: 2px; flex-wrap: wrap; justify-content: center;",
                CurveChip { parent: parent.clone(), t: t.clone(), color: color.clone() }
                OffChip { parent: parent.clone(), t: t.clone() }
                if t.enter >= 0.0 {
                    EnterChip { parent: parent.clone(), t: t.clone() }
                }
            }
        }
    }
}

/// Which end of a range a pointer means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    Lo,
    Hi,
}

/// What a pointer at `(x, y)` on a 36 px range knob is on: the nearer of
/// the two handles along the arc (`lo` and `hi` are their places, 0..1),
/// or `None` on the body (the cap in the middle).
#[must_use]
pub fn pick_handle(x: f64, y: f64, lo: f64, hi: f64) -> Option<Handle> {
    let (dx, dy) = (x - 18.0, y - 18.0);
    if dx.hypot(dy) < 9.0 {
        return None;
    }
    // Degrees clockwise from 3 o'clock, as the arc is drawn; the arc runs
    // from 135° (7:30) through 270° to 405° (4:30).
    let a = (dy.atan2(dx).to_degrees() - 135.0).rem_euclid(360.0);
    let n = if a <= 270.0 {
        a / 270.0
    } else if a - 270.0 < 45.0 {
        1.0
    } else {
        0.0
    };
    Some(if (n - lo).abs() <= (n - hi).abs() { Handle::Lo } else { Handle::Hi })
}

/// A value's place on the range knob's arc, 0..1 of the param's range (by
/// ratio for times and frequencies).
fn arc_pos(t: &MacroTuneView, v: f32) -> f64 {
    let (lo, hi) = (t.min, t.max);
    let n = if t.log && lo > 0.0 && hi > lo {
        (v.max(lo) / lo).ln() / (hi / lo).ln()
    } else if hi > lo {
        (v - lo) / (hi - lo)
    } else {
        0.0
    };
    f64::from(n.clamp(0.0, 1.0))
}

/// [`arc_pos`]'s inverse.
fn arc_value(t: &MacroTuneView, n: f64) -> f32 {
    let n = n.clamp(0.0, 1.0) as f32;
    let (lo, hi) = (t.min, t.max);
    if t.log && lo > 0.0 && hi > lo { lo * (hi / lo).powf(n) } else { n.mul_add(hi - lo, lo) }
}

/// The range knob: the param's whole range as the track, the tuned range
/// in the knob's colour, the patch's value as a tick (not draggable), and a
/// handle at each end. Drag grabs the nearer handle (Shift: fine); the
/// handle under the pointer or being dragged is drawn larger and ringed.
/// Double-click a handle: that end back to what the presets say; the body:
/// the whole response. Hovered, the arrows nudge the handle under the
/// pointer and Backspace resets it.
#[component]
fn TuneKnob(parent: String, t: MacroTuneView, color: String, dragging: Signal<bool>, highlight: bool) -> Element {
    let wire = use_context::<Wire>();
    let bus = DragBus::try_use();
    let mut hover = use_signal(move || highlight.then_some(Handle::Hi));
    let mut active = use_signal(|| None::<Handle>);
    let mut mounted = use_signal(|| None::<std::rc::Rc<MountedData>>);
    let (size, center, radius) = (36.0f64, 18.0f64, 14.0f64);
    let (plo, phi, pbase) = (arc_pos(&t, t.lo), arc_pos(&t, t.hi), arc_pos(&t, t.base));
    let track = arc_path(center, center, radius, angle_for_value(0.0), angle_for_value(1.0));
    let (a, b) = if plo <= phi { (plo, phi) } else { (phi, plo) };
    let range = if b - a > 0.002 {
        arc_path(center, center, radius, angle_for_value(a), angle_for_value(b))
    } else {
        String::new()
    };
    let ink = if t.off { "#52525b".to_string() } else { color.clone() };
    let (tx, ty) = arc_point(center, center, radius + 2.5, angle_for_value(pbase));
    let (tx2, ty2) = arc_point(center, center, radius - 4.0, angle_for_value(pbase));
    let (lx, ly) = arc_point(center, center, radius, angle_for_value(plo));
    let (hx, hy) = arc_point(center, center, radius, angle_for_value(phi));
    let lit = |h: Handle| active() == Some(h) || (active().is_none() && hover() == Some(h));
    let (lr, hr) = (if lit(Handle::Lo) { 4.5 } else { 3.0 }, if lit(Handle::Hi) { 4.5 } else { 3.0 });
    let (lring, hring) = (
        if lit(Handle::Lo) { "#f4f4f5" } else { ink.as_str() },
        if lit(Handle::Hi) { "#f4f4f5" } else { "#18181b" },
    );
    let op_of = |h: Handle| if h == Handle::Lo { "min" } else { "max" };
    let reset_of = |h: Handle| if h == Handle::Lo { "reset_min" } else { "reset_max" };
    let (t_down, t_dbl, t_key) = (t.clone(), t.clone(), t.clone());
    let (w_down, w_dbl, w_key) = (wire.clone(), wire.clone(), wire.clone());
    let (p_down, p_dbl, p_key) = (parent.clone(), parent.clone(), parent.clone());
    rsx! {
        div {
            tabindex: "0",
            style: "width: 36px; height: 36px; position: relative; cursor: ns-resize; touch-action: none; outline: none;",
            title: "Drag a handle (Shift: fine) · double-click a handle to reset that end, the middle to reset all · arrows nudge, Backspace resets",
            onmounted: move |e| mounted.set(Some(e.data())),
            onmouseenter: move |_| {
                if let Some(m) = mounted.peek().clone() {
                    spawn(async move {
                        let _ = m.set_focus(true).await;
                    });
                }
            },
            onmousemove: move |e: MouseEvent| {
                let p = e.element_coordinates();
                hover.set(pick_handle(p.x, p.y, plo, phi));
            },
            onmouseleave: move |_| hover.set(None),
            onpointerdown: move |e: PointerEvent| {
                let Some(bus) = bus else { return };
                let p = e.element_coordinates();
                let Some(h) = pick_handle(p.x, p.y, plo, phi) else { return };
                let fine = e.modifiers().contains(Modifiers::SHIFT);
                let y0 = e.client_coordinates().y;
                let start = if h == Handle::Lo { plo } else { phi };
                let (t, w, parent) = (t_down.clone(), w_down.clone(), p_down.clone());
                let (mut dragging, mut active) = (dragging, active);
                dragging.set(true);
                active.set(Some(h));
                bus.begin(move |ev| match ev {
                    DragEvent::Move { y, .. } => {
                        let scale = if fine { 0.1 } else { 1.0 };
                        let v = arc_value(&t, start + (y0 - y) / SENSITIVITY * scale);
                        w.tune(&parent, &t, op_of(h), v);
                    }
                    DragEvent::End => {
                        let (mut dragging, mut active) = (dragging, active);
                        dragging.set(false);
                        active.set(None);
                    }
                });
            },
            ondoubleclick: move |e: MouseEvent| {
                let p = e.element_coordinates();
                match pick_handle(p.x, p.y, plo, phi) {
                    Some(h) => w_dbl.tune_op(&p_dbl, &t_dbl, reset_of(h), ""),
                    None => w_dbl.tune_op(&p_dbl, &t_dbl, "reset", ""),
                }
            },
            onkeydown: move |e: KeyboardEvent| {
                let h = active().or(hover());
                let step = if e.modifiers().contains(Modifiers::SHIFT) { 0.002 } else { 0.01 };
                let nudge = match e.key() {
                    Key::ArrowUp | Key::ArrowRight => step,
                    Key::ArrowDown | Key::ArrowLeft => -step,
                    Key::Backspace | Key::Delete => {
                        e.prevent_default();
                        match h {
                            Some(h) => w_key.tune_op(&p_key, &t_key, reset_of(h), ""),
                            None => w_key.tune_op(&p_key, &t_key, "reset", ""),
                        }
                        return;
                    }
                    _ => return,
                };
                let Some(h) = h else { return };
                e.prevent_default();
                let at = if h == Handle::Lo { plo } else { phi };
                w_key.tune(&p_key, &t_key, op_of(h), arc_value(&t_key, at + nudge));
            },
            svg {
                width: "36",
                height: "36",
                view_box: "0 0 {size} {size}",
                style: "width: 36px; height: 36px; position: absolute; pointer-events: none;",
                path { d: "{track}", fill: "none", stroke: "#374151", stroke_width: "3", stroke_linecap: "round" }
                if !range.is_empty() {
                    path { d: "{range}", fill: "none", stroke: "{ink}", stroke_width: "3", stroke_linecap: "round" }
                }
                circle { cx: "{center}", cy: "{center}", r: "{radius - 5.0}", fill: "#1F2937" }
                // The patch's own value.
                line {
                    x1: "{tx:.1}", y1: "{ty:.1}", x2: "{tx2:.1}", y2: "{ty2:.1}",
                    stroke: "#f4f4f5", stroke_width: "1.5", stroke_linecap: "round",
                }
                // Bottom handle hollow, top handle filled; the live one
                // larger, ringed in white.
                circle { cx: "{lx:.1}", cy: "{ly:.1}", r: "{lr}", fill: "#18181b", stroke: "{lring}", stroke_width: "1.5" }
                circle { cx: "{hx:.1}", cy: "{hy:.1}", r: "{hr}", fill: "{ink}", stroke: "{hring}", stroke_width: "1.5" }
            }
        }
    }
}

/// A chip's look: quiet, or lit in `ink`.
fn chip(ink: &str) -> String {
    format!(
        "padding: 0 4px; height: 13px; display: flex; align-items: center; border-radius: 4px; \
         border: 1px solid #3f3f46; font-size: 8px; font-weight: 700; cursor: pointer; \
         font-family: ui-monospace, monospace; color: {ink};"
    )
}

/// The curve between rest and each end: a click steps lin → log → exp →
/// S, a right-click picks one. Grey on a default (the engine's own, the
/// drive journey's, a seed), the knob's colour once a preset or the tuning
/// shapes it.
#[component]
fn CurveChip(parent: String, t: MacroTuneView, color: String) -> Element {
    const CURVES: [(&str, &str); 4] = [("lin", "Linear"), ("log", "Log — by ratio"), ("exp", "Exp — slow, then fast"), ("s", "S — slow at both ends")];
    let wire = use_context::<Wire>();
    let host = signal_widgets::PopupHost::try_use();
    let next = CURVES
        .iter()
        .position(|(c, _)| *c == t.curve)
        .map_or("lin", |i| CURVES[(i + 1) % CURVES.len()].0);
    let who = match t.source.as_str() {
        "module" => "the module snapshot",
        "block" => "the block preset",
        "seed" => "the preset's default",
        "tuning" => "this tuning (not saved)",
        "stage" => "the drive journey's default",
        _ => "the macro's own response",
    };
    let label = if t.curve == "s" { "S".to_string() } else { t.curve.clone() };
    let ink = if matches!(t.source.as_str(), "module" | "block" | "tuning") { color.clone() } else { "#71717a".to_string() };
    let (w1, t1, p1) = (wire.clone(), t.clone(), parent.clone());
    let items: Vec<crate::kit::MenuItem> = CURVES
        .iter()
        .map(|(c, l)| crate::kit::MenuItem { checked: *c == t.curve, ..crate::kit::MenuItem::run(c, *l) })
        .collect();
    rsx! {
        div {
            title: "Curve: {label} — shaped by {who}. Click for {next}, right-click to pick.",
            style: chip(&ink),
            onclick: move |e: MouseEvent| {
                e.stop_propagation();
                w1.tune_op(&p1, &t1, "curve", next);
            },
            oncontextmenu: move |e: MouseEvent| {
                e.prevent_default();
                e.stop_propagation();
                let (w, t, p) = (wire.clone(), t.clone(), parent.clone());
                crate::kit::context_menu(
                    host,
                    &e,
                    items.clone(),
                    EventHandler::new(move |x: crate::kit::Picked| w.tune_op(&p, &t, "curve", x.id)),
                );
            },
            "{label}"
        }
    }
}

/// Off: keep the macro off this param altogether. Lit while the macro
/// moves it; grey, reading "off", once it is kept off.
#[component]
fn OffChip(parent: String, t: MacroTuneView) -> Element {
    let wire = use_context::<Wire>();
    let off = t.off;
    rsx! {
        div {
            title: if off { "Kept off this param — click to let the macro move it" } else { "Click to keep the macro off this param" },
            style: if off {
                "padding: 0 4px; height: 13px; display: flex; align-items: center; border-radius: 4px; \
                 background: #3f3f46; font-size: 8px; font-weight: 700; cursor: pointer; \
                 font-family: ui-monospace, monospace; color: #a1a1aa;".to_string()
            } else {
                chip("#a1a1aa")
            },
            onclick: move |e: MouseEvent| {
                e.stop_propagation();
                wire.tune(&parent, &t, "off", if off { 0.0 } else { 1.0 });
            },
            if off { "off" } else { "on" }
        }
    }
}

/// A drive stage's entry point on the Drive knob's upper half: drag it
/// sideways, or right-click for an exact value.
#[component]
fn EnterChip(parent: String, t: MacroTuneView) -> Element {
    let wire = use_context::<Wire>();
    let bus = DragBus::try_use();
    let host = signal_widgets::PopupHost::try_use();
    const STEPS: [(&str, f32); 10] = [
        ("e0", 0.0), ("e10", 0.1), ("e20", 0.2), ("e30", 0.3), ("e40", 0.4),
        ("e50", 0.5), ("e60", 0.6), ("e70", 0.7), ("e80", 0.8), ("e90", 0.9),
    ];
    let pct = (t.enter * 100.0).round();
    let items: Vec<crate::kit::MenuItem> = STEPS
        .iter()
        .map(|(id, v)| crate::kit::MenuItem {
            checked: (v - t.enter).abs() < 0.005,
            ..crate::kit::MenuItem::run(id, format!("Comes in {:.0}% up", v * 100.0))
        })
        .collect();
    let (w_drag, t_drag, p_drag) = (wire.clone(), t.clone(), parent.clone());
    rsx! {
        div {
            title: "Comes in {pct}% of the way up the Drive knob — drag sideways, or right-click for a value",
            style: format!("{} cursor: ew-resize; touch-action: none;", chip("#a1a1aa")),
            onpointerdown: move |e: PointerEvent| {
                e.stop_propagation();
                let Some(bus) = bus else { return };
                let x0 = e.client_coordinates().x;
                let start = f64::from(t_drag.enter.max(0.0));
                let (w, t, p) = (w_drag.clone(), t_drag.clone(), p_drag.clone());
                bus.begin(move |ev| {
                    if let DragEvent::Move { x, .. } = ev {
                        let v = (start + (x - x0) / SENSITIVITY).clamp(0.0, 0.99) as f32;
                        w.tune(&p, &t, "enter", v);
                    }
                });
            },
            oncontextmenu: move |e: MouseEvent| {
                e.prevent_default();
                e.stop_propagation();
                let (w, t, p) = (wire.clone(), t.clone(), parent.clone());
                crate::kit::context_menu(
                    host,
                    &e,
                    items.clone(),
                    EventHandler::new(move |x: crate::kit::Picked| {
                        if let Some((_, v)) = STEPS.iter().find(|(id, _)| *id == x.id) {
                            w.tune(&p, &t, "enter", *v);
                        }
                    }),
                );
            },
            "in {pct}%"
        }
    }
}

// ============================================================================
// DualRowDropdown — Delay / Reverb: a header row, a row per block, and
// links for Type and Time
// ============================================================================

#[component]
fn DualRowDropdown(
    /// Knob id prefix, `delay` or `reverb` — for the link mirror lookups.
    prefix: String,
    /// The five column headers.
    headers: Vec<String>,
    children_knobs: Vec<MacroChildView>,
    dragging: Signal<bool>,
) -> Element {
    let mut type_linked = use_signal(|| false);
    // One row per block, in the columns the headers name.
    let mut rows: Vec<(String, Vec<MacroChildView>)> = Vec::new();
    for c in &children_knobs {
        match rows.iter_mut().find(|(g, _)| *g == c.group) {
            Some((_, r)) => r.push(c.clone()),
            None => rows.push((c.group.clone(), vec![c.clone()])),
        }
    }
    // The columns, by the knobs' ids (`delay-fb1`): a block without one
    // leaves its cell empty rather than shifting the row.
    let keys: [&str; 5] = if prefix == "reverb" {
        // `reverb-time` is the decay: how much tail, not when.
        ["type", "time", "character", "level", "mod"]
    } else {
        ["type", "fb", "filter", "level", "mod"]
    };
    let cols = |row: &[MacroChildView]| -> Vec<Option<MacroChildView>> {
        keys.iter()
            .map(|key| {
                let stem = format!("{prefix}-{key}");
                row.iter()
                    .find(|c| c.id.strip_prefix(&stem).is_some_and(|n| n.chars().all(|ch| ch.is_ascii_digit())))
                    .cloned()
            })
            .collect()
    };
    let cell = |c: Option<MacroChildView>| {
        let prefix = prefix.clone();
        match c {
            Some(c) => {
                let mirror = linked_mirror(&c.id, &prefix, type_linked());
                rsx! { ChildCell { key: "{c.id}", child: c.clone(), dragging, width: 0, mirror } }
            }
            None => rsx! { div {} },
        }
    };
    let link = |on: bool| {
        format!(
            "display: flex; align-items: center; justify-content: center; padding: 2px 4px; border-radius: 4px; \
             cursor: pointer; background: {};",
            if on { "rgba(22,78,99,0.4)" } else { "transparent" },
        )
    };

    rsx! {
        div { style: "display: grid; grid-template-columns: repeat(5, 68px); column-gap: 4px; row-gap: 0;",
            // ── Header row ──
            for header in headers.iter() {
                div {
                    style: "text-align: center; font-size: 9px; font-weight: 600; color: #71717a; \
                            text-transform: uppercase; letter-spacing: 0.05em; padding: 4px 0;",
                    "{header}"
                }
            }
            for (ri, (_, row)) in rows.iter().enumerate() {
                if ri == 1 {
                    // ── Link buttons row ──
                    div { style: "display: flex; align-items: center; justify-content: center; padding: 2px 0;",
                        div {
                            title: "Link Type 1 and 2",
                            style: link(type_linked()),
                            onclick: move |_| type_linked.set(!type_linked()),
                            LinkGlyph { on: type_linked() }
                        }
                    }
                    div {}
                    div {}
                    div {}
                    div {}
                }
                for c in cols(row) {
                    {cell(c)}
                }
            }
        }
    }
}

/// The legacy 🔗, drawn: Blitz has no colour-emoji font to draw it with.
#[component]
fn LinkGlyph(on: bool) -> Element {
    let stroke = if on { "#22d3ee" } else { "#52525b" };
    rsx! {
        svg {
            width: "12",
            height: "12",
            view_box: "0 0 24 24",
            style: "width: 12px; height: 12px;",
            path {
                d: "M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71",
                fill: "none",
                stroke: "{stroke}",
                stroke_width: "2.2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
            }
            path {
                d: "M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71",
                fill: "none",
                stroke: "{stroke}",
                stroke_width: "2.2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
            }
        }
    }
}

// ============================================================================
// GroupedDropdown — Pitch / Clarity: the knobs by the block they turn, a
// quiet header over each block's row, two blocks to a line
// ============================================================================

#[component]
fn GroupedDropdown(children_knobs: Vec<MacroChildView>, dragging: Signal<bool>) -> Element {
    let mut groups: Vec<(String, Vec<MacroChildView>)> = Vec::new();
    for c in &children_knobs {
        match groups.iter_mut().find(|(g, _)| *g == c.group) {
            Some((_, r)) => r.push(c.clone()),
            None => groups.push((c.group.clone(), vec![c.clone()])),
        }
    }
    // Two blocks to a column, filled down then across — DLY 1 over DLY 2,
    // VERB 1 over VERB 2 — keeps the panel short enough to hang under the
    // bar (four rows of Clarity would run off the bottom of the window) and
    // narrow enough to fit from the Delay knob to the end of the row.
    let rows = groups.len().min(2);
    rsx! {
        div {
            style: "display: grid; grid-template-rows: repeat({rows}, auto); grid-auto-flow: column; \
                    grid-auto-columns: max-content; column-gap: 16px; row-gap: 6px; align-items: start;",
            for (group, row) in groups.iter() {
                div { key: "{group}", style: "display: flex; flex-direction: column; gap: 0;",
                    div {
                        style: "font-size: 9px; font-weight: 600; color: #71717a; text-transform: uppercase; \
                                letter-spacing: 0.05em; white-space: nowrap; padding: 2px 2px 0;",
                        "{group}"
                    }
                    // Bottom-aligned, so a cell with an ON/OFF pad over it
                    // keeps its label and knob on the line of the others.
                    div { style: "display: flex; align-items: flex-end; gap: 4px;",
                        for c in row.iter() {
                            ChildCell { key: "{c.id}", child: c.clone(), dragging, width: 64 }
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// MiniKnob — the legacy mini knob (36 px, arc + pointer), on the drag bus
// ============================================================================

/// The legacy `MiniKnob`: a 36 px arc knob — a grey track, the value arc
/// in the knob's colour (from the centre when bipolar), a dark cap and a
/// pointer. `spread` is Width's: at 0 a single mark at 12 o'clock, the arc
/// opening both ways from it as the value rises. Drags ride the rig root's
/// drag bus (Blitz has no pointer capture); wheel steps 1 %; a double-click
/// goes back to rest.
#[component]
fn MiniKnob(
    value: f32,
    color: String,
    #[props(default)] bipolar: bool,
    #[props(default)] spread: bool,
    /// Where a double-click puts it back: the patch as dialled.
    #[props(default = 0.5)]
    rest: f32,
    /// Set while a drag is live.
    dragging: Signal<bool>,
    on_change: Callback<f32>,
    /// Double-click: back to rest, as the rig decides it (a panel knob to
    /// where its bar knob puts it). Without one, to `rest` here.
    #[props(default)]
    on_reset: Option<Callback<()>>,
    /// Nothing to turn (an empty drive slot).
    #[props(default)]
    disabled: bool,
) -> Element {
    let bus = DragBus::try_use();
    let mut shield = use_signal(|| None::<(f64, f64)>);
    let display = value.clamp(0.0, 1.0);
    let v = f64::from(display);

    let size: f64 = 36.0;
    let center: f64 = size / 2.0;
    let radius: f64 = 14.0;
    let track_path = arc_path(center, center, radius, angle_for_value(0.0), angle_for_value(1.0));
    let value_path = if spread {
        if v > 0.001 {
            arc_path(center, center, radius, angle_for_value(0.5 - v / 2.0), angle_for_value(0.5 + v / 2.0))
        } else {
            String::new()
        }
    } else if bipolar {
        if v > 0.501 {
            arc_path(center, center, radius, angle_for_value(0.5), angle_for_value(v))
        } else if v < 0.499 {
            arc_path(center, center, radius, angle_for_value(v), angle_for_value(0.5))
        } else {
            String::new()
        }
    } else if v > 0.001 {
        arc_path(center, center, radius, angle_for_value(0.0), angle_for_value(v))
    } else {
        String::new()
    };
    // Centre tick at 12 o'clock (bipolar), or Width's mono mark.
    let (tick_x, tick_y) = arc_point(center, center, radius + 2.0, angle_for_value(0.5));
    let (tick_x2, tick_y2) = arc_point(center, center, radius - if spread { 5.0 } else { 1.0 }, angle_for_value(0.5));
    let (px, py) = arc_point(center, center, radius - 3.0, angle_for_value(v));

    let apply = move |n: f64| on_change.call(n.clamp(0.0, 1.0) as f32);

    rsx! {
        div {
            style: "width: 36px; height: 36px; position: relative; cursor: pointer; touch-action: none;",
            onpointerdown: move |e: PointerEvent| {
                if disabled {
                    return;
                }
                let y0 = e.client_coordinates().y;
                let mut dragging = dragging;
                dragging.set(true);
                // A click (or the first of a double-click) is not a turn:
                // nothing moves until the pointer has travelled a few pixels.
                let moved = std::cell::Cell::new(false);
                match bus {
                    Some(bus) => bus.begin(move |ev| match ev {
                        DragEvent::Move { y, .. } => {
                            if !moved.get() && (y0 - y).abs() < 3.0 {
                                return;
                            }
                            moved.set(true);
                            apply(v + (y0 - y) / SENSITIVITY);
                        }
                        DragEvent::End => {
                            let mut dragging = dragging;
                            dragging.set(false);
                        }
                    }),
                    None => shield.set(Some((y0, v))),
                }
            },
            onwheel: move |e: WheelEvent| {
                if disabled {
                    return;
                }
                let step = if e.modifiers().contains(Modifiers::SHIFT) { 0.002 } else { 0.01 };
                let up = e.delta().strip_units().y < 0.0;
                apply(if up { v + step } else { v - step });
            },
            ondoubleclick: move |_| {
                if disabled {
                    return;
                }
                match on_reset {
                    Some(r) => r.call(()),
                    None => apply(f64::from(rest)),
                }
            },
            svg {
                width: "36",
                height: "36",
                view_box: "0 0 {size} {size}",
                style: "width: 36px; height: 36px; position: absolute; pointer-events: none;",
                // Background track arc
                path { d: "{track_path}", fill: "none", stroke: "#374151", stroke_width: "3", stroke_linecap: "round" }
                // Value arc
                if !value_path.is_empty() {
                    path { d: "{value_path}", fill: "none", stroke: "{color}", stroke_width: "3", stroke_linecap: "round" }
                }
                if bipolar {
                    line {
                        x1: "{tick_x:.1}", y1: "{tick_y:.1}", x2: "{tick_x2:.1}", y2: "{tick_y2:.1}",
                        stroke: "#6B7280", stroke_width: "1.5", stroke_linecap: "round",
                    }
                }
                // Center circle
                circle { cx: "{center}", cy: "{center}", r: "{radius - 4.0}", fill: "#1F2937" }
                if spread {
                    // Width reads from the top: its mark stays at 12.
                    line {
                        x1: "{tick_x:.1}", y1: "{tick_y:.1}", x2: "{tick_x2:.1}", y2: "{tick_y2:.1}",
                        stroke: "{color}", stroke_width: "2", stroke_linecap: "round",
                    }
                } else {
                    // Pointer
                    line {
                        x1: "{center}", y1: "{center}", x2: "{px:.1}", y2: "{py:.1}",
                        stroke: "{color}", stroke_width: "2", stroke_linecap: "round",
                    }
                }
            }
            // No drag bus above (a test mounting the bar alone): a local
            // shield owns the pointer while the drag is live.
            if shield().is_some() {
                div {
                    style: "position: absolute; inset: -2000px; z-index: 1000; cursor: ns-resize;",
                    onpointermove: move |e: PointerEvent| {
                        if let Some((y0, v0)) = shield() {
                            apply(v0 + (y0 - e.client_coordinates().y) / SENSITIVITY);
                        }
                    },
                    onpointerup: move |_| {
                        shield.set(None);
                        let mut dragging = dragging;
                        dragging.set(false);
                    },
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::wasm_bindgen_test as test;

    /// Grabbing a range knob takes the nearer handle along the arc; the cap
    /// in the middle is neither.
    #[test]
    fn a_drag_grabs_the_nearer_handle() {
        use super::{Handle, pick_handle};
        // Range 0.2..0.8. At 7:30 (the arc's start) the bottom handle is
        // nearer; at 4:30 (its end) the top one; at 12 o'clock (0.5) a tie
        // goes to the bottom.
        let at = |deg: f64| {
            let r: f64 = 15.0;
            (r.mul_add(deg.to_radians().cos(), 18.0), r.mul_add(deg.to_radians().sin(), 18.0))
        };
        let (x, y) = at(135.0);
        assert_eq!(pick_handle(x, y, 0.2, 0.8), Some(Handle::Lo));
        let (x, y) = at(45.0);
        assert_eq!(pick_handle(x, y, 0.2, 0.8), Some(Handle::Hi));
        let (x, y) = at(300.0); // just right of 12
        assert_eq!(pick_handle(x, y, 0.2, 0.8), Some(Handle::Hi));
        // In the gap at the bottom: whichever end is closer.
        let (x, y) = at(100.0);
        assert_eq!(pick_handle(x, y, 0.2, 0.8), Some(Handle::Lo));
        let (x, y) = at(80.0);
        assert_eq!(pick_handle(x, y, 0.2, 0.8), Some(Handle::Hi));
        // An inverted range (bottom above top) still picks by nearness.
        let (x, y) = at(45.0);
        assert_eq!(pick_handle(x, y, 0.9, 0.1), Some(Handle::Lo));
        // The body.
        assert_eq!(pick_handle(18.0, 20.0, 0.2, 0.8), None);
    }

    /// The Type link pairs the two rows' Type knobs — nothing else links.
    #[test]
    fn links_mirror_type_and_time_only() {
        assert_eq!(linked_mirror("delay-type1", "delay", true).as_deref(), Some("delay-type2"));
        assert_eq!(linked_mirror("delay-type2", "delay", true).as_deref(), Some("delay-type1"));
        assert_eq!(linked_mirror("delay-type1", "delay", false), None, "unlinked");
        assert_eq!(linked_mirror("delay-fb1", "delay", true), None, "only Type links");
        assert_eq!(linked_mirror("delay-type1", "reverb", true), None, "its own panel only");
    }

    fn child(fmt: &str, param: f32) -> MacroChildView {
        MacroChildView {
            id: "x".into(),
            label: "X".into(),
            color: String::new(),
            value: 0.5,
            rest: 0.5,
            group: String::new(),
            has_pad: false,
            bypassed: false,
            fmt: fmt.into(),
            param,
            aux: 0.0,
            steps: 0,
            slot: String::new(),
            subtitle: String::new(),
            tooltip: String::new(),
            empty: false,
        }
    }

    /// Readouts in the rig's units: intervals signed with a real minus,
    /// the Ice menu's cents and Free, divisions, dB, Hz.
    #[test]
    fn readouts_speak_the_rigs_units() {
        assert_eq!(child_readout(&child("semitones", 12.0)), "+12");
        assert_eq!(child_readout(&child("semitones", -12.0)), "\u{2212}12");
        assert_eq!(child_readout(&child("semitones", 7.0)), "+7");
        assert_eq!(child_readout(&child("interval", 27.0)), "+12");
        assert_eq!(child_readout(&child("interval", 0.0)), "\u{2212}12");
        assert_eq!(child_readout(&child("interval", 14.0)), "+25c");
        assert_eq!(child_readout(&child("interval", 30.0)), "Free");
        assert_eq!(child_readout(&child("div", 1.0)), "1/8.");
        assert_eq!(child_readout(&child("db", -60.0)), "Off");
        assert_eq!(child_readout(&child("hz", 250.0)), "250 Hz");
        assert_eq!(child_readout(&child("ms", 1500.0)), "1.50 s");
        assert_eq!(child_readout(&child("pct", 0.42)), "42%");
        assert_eq!(fmt_value("pan", -0.4, 0.0), "L40");
        assert_eq!(fmt_value("pan", 0.0, 0.0), "C");
    }
}
