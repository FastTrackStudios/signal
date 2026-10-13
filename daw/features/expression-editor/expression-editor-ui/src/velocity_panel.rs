//! The velocity panel.
//!
//! One component, two homes: `examples/panel.rs` runs it in a desktop
//! window for iteration, and `fts-extensions` registers it as a REAPER
//! panel rendered by Blitz. Propless is what allows both — the sink
//! arrives through context, so the same component works with a real
//! project behind it or with nothing at all.
//!
//! ## Layout, and why it differs from MVelocity
//!
//! Upstream is four boxes of sliders with no shared readout: you move
//! something and look at the MIDI editor to find out what happened. Here
//! the top of the panel is a live preview of the *resolved* take — every
//! engine applied — and the four sections sit under it. You are always
//! looking at the thing you're editing.
//!
//! The sections are laid out in chain order, top to bottom: curve
//! (shape), step (accent), randomize (humanize), compress (glue). That
//! matches `velocity::Session`'s pipeline, so reading the panel downward
//! tells you what happens to a note in the order it happens.
//!
//! ## Committing
//!
//! Nothing is written until you press Apply. The panel is a preview
//! surface; a tool that writes on every slider frame turns one gesture
//! into hundreds of undo points, and MVelocity's habit of writing
//! continuously is exactly why its undo history is unusable. Revert puts
//! the take back the way it was found, which is meaningful even after an
//! Apply — the baseline is held for the life of the session.

use std::sync::Arc;

use dioxus::prelude::*;
use expression_editor_tools::velocity::{
    CurvePreset, MAX_VELOCITY, MIN_VELOCITY, Pivot, Range, Session,
};
use expression_editor_tools::{DemoSink, VelocitySink};

use crate::curve_editor::CurveEditor;
use crate::drag::{BarEditor, RangeSlider, Slider};

// architect-ui is OPTIONAL here (behind `native`, paired with
// nice-plug-dioxus), but PanelStyles is not cfg-gated — so this cannot be
// `architect_ui::THEME_CSS` the way the unconditional consumers do it.
// Read the vendored copy instead. Canonical source is architect-ui's
// assets/fts-theme.css; libs/ui/assets/ is a copy kept in step by hand.
const FTS_THEME: &str = include_str!("../../../../libs/ui/assets/fts-theme.css");

/// Without `html,body{height:100%}` a full-height root resolves against
/// `auto` and the panel collapses to its content. Mirrors the reset
/// `apps/fasttrackstudio` injects.
const HOST_RESET: &str = r#"
html, body { margin:0; padding:0; height:100%; width:100%; overflow:hidden; }
* { box-sizing: border-box; }
body > div { height:100%; }
"#;

/// Cloneable handle so a sink can live in Dioxus context.
///
/// Provide one with `use_context_provider`; the panel falls back to
/// [`DemoSink`] when nothing is provided, which is what makes the
/// standalone example work with no DAW at all.
#[derive(Clone)]
pub struct SinkHandle(pub Arc<dyn VelocitySink>);

impl SinkHandle {
    pub fn new(sink: impl VelocitySink) -> Self {
        Self(Arc::new(sink))
    }
}

impl PartialEq for SinkHandle {
    /// Identity, not contents — a sink is a service, and re-rendering
    /// shouldn't depend on comparing one.
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// The MIDI velocity tool.
#[component]
pub fn VelocityPanel() -> Element {
    // `try_consume_context`, not `use_context`: the latter panics when
    // nothing was provided, and "no sink" is a supported configuration —
    // it's what the standalone example runs in.
    let sink = use_context_provider(|| {
        try_consume_context::<SinkHandle>().unwrap_or_else(|| SinkHandle::new(DemoSink::default()))
    });

    // `Session` is the whole model. Every control writes into it and the
    // preview is a pure function of it, so there is no second copy of the
    // state to keep in step.
    let mut session = use_signal(|| sink.0.open().unwrap_or_else(|_| Session::default()));
    let mut status = use_signal(|| match sink.0.open() {
        Ok(s) => format!("{} notes", s.baseline().len()),
        Err(e) => e,
    });

    // Memoized: `resolve` runs all four engines over every note, and the
    // panel re-renders on every frame of a slider drag. Recomputing it
    // per render is fine for the 32-note demo take and visibly janky on a
    // real one — a few hundred notes times four engines, sixty times a
    // second, for a value that only changes when the session does.
    let resolved = use_memo(move || session.read().resolve());
    let pending = use_memo(move || session.read().edits().len());

    let resolved = resolved();
    let pending = pending();
    let selected = resolved.iter().filter(|n| n.selected).count();

    // Re-opening rebinds to whatever is selected now, discarding the
    // parameters — a deliberate reset, distinct from the silent resync
    // the sink offers.
    let reopen = {
        let sink = sink.clone();
        move |_| match sink.0.open() {
            Ok(s) => {
                status.set(format!("{} notes", s.baseline().len()));
                session.set(s);
            }
            Err(e) => status.set(e),
        }
    };

    // Live: every parameter change writes straight through to the take.
    //
    // `use_effect` re-runs whenever the session it reads changes, and
    // `commit` doesn't mutate the session, so there's no feedback loop.
    // This is safe to do on every change specifically because
    // `Session::edits()` is a diff against the baseline — a slider at
    // neutral writes nothing, and a drag only rewrites the notes whose
    // velocity actually moved.
    {
        let sink = sink.clone();
        use_effect(move || {
            let session = session.read();
            if session.is_empty() {
                return;
            }
            match sink.0.commit(&session) {
                Ok(0) => status.set(format!("{} notes", session.baseline().len())),
                Ok(n) => status.set(format!("live · {n} notes changed")),
                Err(e) => status.set(e),
            }
        });
    }

    let revert = {
        let sink = sink.clone();
        move |_| {
            // Bind the result before writing: holding the read guard
            // across `session.write()` is a runtime borrow panic.
            let result = sink.0.revert(&session.read());
            match result {
                Ok(n) => {
                    session.write().reset();
                    status.set(format!("reverted {n} notes"));
                }
                Err(e) => status.set(e),
            }
        }
    };

    rsx! {
        PanelStyles {}

        div {
            style: "display:flex; flex-direction:column; gap:10px; height:100%; padding:12px; overflow-y:auto; background:var(--background, #121212); color:var(--foreground, #e8e8e8); font-family:system-ui, sans-serif; font-size:12px;",

            // ── Header ────────────────────────────────────────────
            div {
                style: "display:flex; align-items:baseline; justify-content:space-between; gap:8px;",
                div { style: "font-size:13px; font-weight:600; letter-spacing:0.04em;", "MIDI VELOCITY" }
                div {
                    style: "opacity:0.65;",
                    if selected > 0 {
                        "{resolved.len()} notes · {selected} selected"
                    } else {
                        "{resolved.len()} notes"
                    }
                }
            }

            // ── Preview + curve ───────────────────────────────────
            CurveEditor {
                resolved: resolved.clone(),
                curve: session.read().curve.clone(),
                on_curve: move |c| session.write().curve = Some(c),
            }

            Section { title: "CURVE",
                div {
                    style: "display:flex; flex-wrap:wrap; gap:4px;",
                    for preset in CurvePreset::ALL {
                        Chip {
                            label: preset.label().to_string(),
                            active: false,
                            testid: format!("curve-{}", preset.label().to_lowercase().replace(' ', "-")),
                            onclick: move |_| session.write().curve = Some(preset.curve()),
                        }
                    }
                    Chip {
                        label: "Invert".to_string(),
                        active: false,
                        onclick: move |_| {
                            let mut s = session.write();
                            if let Some(c) = s.curve.as_mut() { c.invert(); }
                        },
                    }
                    Chip {
                        label: "Clear".to_string(),
                        active: false,
                        onclick: move |_| session.write().curve = None,
                    }
                }
            }

            // ── Step velocity ─────────────────────────────────────
            Section { title: "STEP VELOCITY",
                div {
                    style: "display:flex; align-items:flex-end; gap:8px;",
                    div {
                        style: "flex:1; min-width:120px;",
                        BarEditor {
                            testid: "pattern-bars".to_string(),
                            values: session.read().pattern.steps().to_vec(),
                            on_change: move |(i, v)| session.write().pattern.set(i, v),
                        }
                    }
                    div {
                        style: "display:flex; flex-direction:column; gap:4px;",
                        div {
                            style: "display:flex; gap:4px;",
                            Chip {
                                label: "−".to_string(),
                                active: false,
                                onclick: move |_| session.write().pattern.pop(),
                            }
                            Chip {
                                label: "+".to_string(),
                                active: false,
                                onclick: move |_| session.write().pattern.push(),
                            }
                        }
                        div { style: "opacity:0.6; text-align:center;", "{session.read().pattern.len()} steps" }
                    }
                }
                Labelled {
                    label: "Amount".to_string(),
                    value: format!("{:.0}%", session.read().pattern_amount * 100.0),
                    Slider {
                        testid: "pattern-amount".to_string(),
                        value: session.read().pattern_amount,
                        min: 0.0,
                        max: 1.0,
                        on_change: move |v| session.write().pattern_amount = v,
                    }
                }
            }

            // ── Randomize ─────────────────────────────────────────
            Section { title: "RANDOMIZE",
                div {
                    style: "display:flex; align-items:center; gap:8px;",
                    Chip {
                        label: "Roll".to_string(),
                        active: false,
                        onclick: move |_| session.write().roll(),
                    }
                    div {
                        style: "opacity:0.6;",
                        if session.read().randomize.is_empty() { "no hand dealt" } else { "hand held" }
                    }
                }
                Labelled {
                    label: "Amount".to_string(),
                    value: format!("{:.0}%", session.read().randomize_amount * 100.0),
                    Slider {
                        testid: "randomize-amount".to_string(),
                        value: session.read().randomize_amount,
                        min: 0.0,
                        max: 1.0,
                        on_change: move |v| session.write().randomize_amount = v,
                    }
                }
            }

            // ── Compress / expand ─────────────────────────────────
            Section { title: "COMPRESS / EXPAND",
                Labelled {
                    label: dynamics_label(session.read().dynamics.amount),
                    value: format!("{:+.0}%", session.read().dynamics.amount * 100.0),
                    Slider {
                        testid: "dynamics-amount".to_string(),
                        value: session.read().dynamics.amount,
                        min: -1.0,
                        max: 1.0,
                        on_change: move |v| session.write().dynamics.amount = v,
                    }
                }
                div {
                    style: "display:flex; align-items:center; gap:6px;",
                    Chip {
                        label: "Mean".to_string(),
                        active: session.read().dynamics.pivot == Pivot::Mean,
                        onclick: move |_| session.write().dynamics.pivot = Pivot::Mean,
                    }
                    Chip {
                        label: "Target".to_string(),
                        active: matches!(session.read().dynamics.pivot, Pivot::Fixed(_)),
                        onclick: move |_| {
                            // Seed the fixed pivot from wherever Mean
                            // currently sits, so switching modes doesn't
                            // jump the result.
                            let mut s = session.write();
                            let at = s.dynamics.pivot_velocity(s.baseline()).round() as u8;
                            s.dynamics.pivot = Pivot::Fixed(at.clamp(MIN_VELOCITY, MAX_VELOCITY));
                        },
                    }
                    if let Pivot::Fixed(v) = session.read().dynamics.pivot {
                        Slider {
                            value: f64::from(v),
                            min: f64::from(MIN_VELOCITY),
                            max: f64::from(MAX_VELOCITY),
                            width: 100.0,
                            on_change: move |x: f64| {
                                session.write().dynamics.pivot = Pivot::Fixed(x.round() as u8);
                            },
                        }
                        div { style: "width:28px; text-align:right; opacity:0.75;", "{v}" }
                    } else {
                        div {
                            style: "opacity:0.6;",
                            "pivot {session.read().dynamics.pivot_velocity(session.read().baseline()).round()}"
                        }
                    }
                }
            }

            // ── Range ─────────────────────────────────────────────
            Section { title: "RANGE",
                div {
                    style: "display:flex; align-items:center; gap:8px;",
                    div { style: "width:24px; opacity:0.75;", "{session.read().range.min}" }
                    RangeSlider {
                        low: f64::from(session.read().range.min),
                        high: f64::from(session.read().range.max),
                        min: f64::from(MIN_VELOCITY),
                        max: f64::from(MAX_VELOCITY),
                        on_change: move |(lo, hi): (f64, f64)| {
                            session.write().range = Range::new(lo.round() as u8, hi.round() as u8);
                        },
                    }
                    div { style: "width:28px; opacity:0.75;", "{session.read().range.max}" }
                    Chip {
                        label: "Full".to_string(),
                        active: false,
                        onclick: move |_| session.write().range = Range::default(),
                    }
                }
            }

            // ── Commit bar ────────────────────────────────────────
            div {
                style: "display:flex; align-items:center; gap:8px; margin-top:auto; padding-top:8px; border-top:1px solid var(--border, #2c2c2c);",
                // No Apply: edits land as you make them. Revert is the
                // counterpart — it restores the baseline the session was
                // opened on, which is the only thing "undo" can mean for
                // a tool whose controls are parameters rather than steps.
                button {
                    style: "{BUTTON} background:var(--primary, #d2691e); color:var(--primary-foreground, #fff); border-color:transparent;",
                    "data-testid": "revert",
                    onclick: revert,
                    "Revert"
                }
                button { style: "{BUTTON}", onclick: reopen, "Reload" }
                div {
                    "data-testid": "status",
                    style: "flex:1; text-align:right; opacity:0.7;",
                    if pending > 0 { "{status} · {pending} pending" } else { "{status}" }
                }
            }
        }
    }
}

/// The panel's stylesheets, isolated in a propless child.
///
/// Not inlined into [`VelocityPanel`]: `document::Style` warns on every
/// re-render ("Changing the props of `Style {}` is not supported"), and
/// the panel re-renders on every frame of a slider drag. A component with
/// no props is memoized, so these mount once and are never diffed again.
#[component]
pub(crate) fn PanelStyles() -> Element {
    rsx! {
        document::Style { {HOST_RESET} }
        document::Style { {FTS_THEME} }
    }
}

/// Says what the bipolar control is currently doing, rather than making
/// you remember which side is which.
fn dynamics_label(amount: f64) -> String {
    if amount < 0.0 {
        "Compress".to_string()
    } else if amount > 0.0 {
        "Expand".to_string()
    } else {
        "Off".to_string()
    }
}

pub(crate) const BUTTON: &str = "padding:5px 12px; border-radius:4px; border:1px solid var(--border, #3a3a3a); background:var(--secondary, #232323); color:inherit; font-size:12px; cursor:pointer;";

/// A titled block. Purely so the four sections look like four sections
/// without each one repeating a wall of inline style.
#[component]
pub(crate) fn Section(title: String, children: Element) -> Element {
    rsx! {
        div {
            style: "display:flex; flex-direction:column; gap:6px; padding:8px; border-radius:5px; border:1px solid var(--border, #2c2c2c); background:var(--card, #181818);",
            div { style: "font-size:10px; letter-spacing:0.09em; opacity:0.6;", "{title}" }
            {children}
        }
    }
}

/// A label / control / readout row, so every slider reports its value.
#[component]
fn Labelled(label: String, value: String, children: Element) -> Element {
    rsx! {
        div {
            style: "display:flex; align-items:center; gap:8px;",
            div { style: "width:64px; opacity:0.75;", "{label}" }
            {children}
            div { style: "width:44px; text-align:right; opacity:0.75;", "{value}" }
        }
    }
}

/// A small toggle/action button.
#[component]
pub(crate) fn Chip(
    label: String,
    active: bool,
    /// Test hook. Empty in production.
    #[props(default)]
    testid: String,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let tone = if active {
        "background:var(--primary, #d2691e); color:var(--primary-foreground, #fff); border-color:transparent;"
    } else {
        "background:var(--secondary, #232323); color:inherit;"
    };
    rsx! {
        button {
            "data-testid": "{testid}",
            style: "padding:3px 8px; border-radius:4px; border:1px solid var(--border, #3a3a3a); font-size:11px; cursor:pointer; {tone}",
            onclick: move |e| onclick.call(e),
            "{label}"
        }
    }
}
