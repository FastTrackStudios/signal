//! Envelope + filter **visualizations** for the layer zoom — the shapes a
//! player reads at a glance, replacing the raw ADSR / cutoff knob rows.
//!
//! Both are driven by the lane's macros and write back through them, so the
//! graph and any knob view stay the same state. Geometry follows the synth
//! rig's editor (Vital-style): time axis stretched by the segment lengths,
//! draggable handles at the segment corners.

use dioxus::prelude::*;

/// ADSR values in macro units (ms / 0..1), as the layer detail carries them.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Adsr {
    pub attack_ms: f32,
    pub decay_ms: f32,
    pub sustain: f32,
    pub release_ms: f32,
}

/// Which handle a drag is moving.
#[derive(Clone, Copy, PartialEq)]
enum Grab {
    Attack,
    Decay,
    Sustain,
    Release,
}

const W: f64 = 260.0;
const H: f64 = 96.0;
const PAD: f64 = 6.0;
/// Seconds of envelope that fill the graph width — segments scale within it.
const FULL_SPAN_MS: f64 = 4000.0;

fn x_for(ms: f64) -> f64 {
    PAD + (ms / FULL_SPAN_MS).clamp(0.0, 1.0) * (W - 2.0 * PAD)
}

fn ms_for(x: f64) -> f32 {
    (((x - PAD) / (W - 2.0 * PAD)).clamp(0.0, 1.0) * FULL_SPAN_MS) as f32
}

/// The envelope shape: attack ramp → decay to sustain → hold → release.
#[component]
pub fn EnvelopeGraph(
    title: String,
    adsr: Adsr,
    #[props(default = "#38bdf8".to_string())] accent: String,
    #[props(default = true)] live: bool,
    /// `(macro-id-suffix, value)` — "attack" | "decay" | "sustain" | "release".
    on_change: EventHandler<(&'static str, f32)>,
) -> Element {
    let mut grab = use_signal(|| None::<Grab>);

    // Geometry: attack ends at xa, decay at xd, sustain holds to xs, release
    // lands at xr. Sustain level sets the plateau height.
    let xa = x_for(adsr.attack_ms as f64);
    let xd = x_for((adsr.attack_ms + adsr.decay_ms) as f64);
    let sus_y = PAD + (1.0 - adsr.sustain.clamp(0.0, 1.0) as f64) * (H - 2.0 * PAD);
    let xs = (xd + 34.0).min(W - PAD - 20.0);
    let xr = (xs + x_for(adsr.release_ms as f64) - PAD).min(W - PAD);
    let y0 = H - PAD;
    let ytop = PAD;

    let path = format!(
        "M {PAD:.1} {y0:.1} L {xa:.1} {ytop:.1} L {xd:.1} {sus_y:.1} L {xs:.1} {sus_y:.1} L {xr:.1} {y0:.1}"
    );
    let fill = format!("{path} L {PAD:.1} {y0:.1} Z");
    let stroke = if live { accent.clone() } else { "#52525b".to_string() };

    rsx! {
        div {
            style: "display: flex; flex-direction: column; gap: 6px; padding: 12px; \
                    border: 1px solid #1f1f23; border-radius: 14px; background: #0e0e11;",
            div { style: "display: flex; align-items: center; gap: 6px;",
                span {
                    style: format!("width: 6px; height: 6px; border-radius: 999px; background: {stroke};"),
                }
                span {
                    style: "font-size: 10px; font-weight: 700; letter-spacing: 0.08em; \
                            text-transform: uppercase; color: #a1a1aa;",
                    "{title}"
                }
                div { style: "flex: 1;" }
                span { style: "font-size: 9px; color: #52525b; font-variant-numeric: tabular-nums;",
                    {format!("A {:.0} · D {:.0} · S {:.2} · R {:.0}", adsr.attack_ms, adsr.decay_ms, adsr.sustain, adsr.release_ms)}
                }
            }
            svg {
                width: "{W}", height: "{H}", view_box: "0 0 {W} {H}",
                style: "touch-action: none; cursor: crosshair;",
                // Grid.
                for frac in [0.25f64, 0.5, 0.75] {
                    line {
                        x1: "{PAD}", y1: "{PAD + frac * (H - 2.0 * PAD)}",
                        x2: "{W - PAD}", y2: "{PAD + frac * (H - 2.0 * PAD)}",
                        stroke: "#1b1b1f", stroke_width: "1",
                    }
                }
                path { d: "{fill}", fill: "{stroke}", fill_opacity: "0.14" }
                path { d: "{path}", fill: "none", stroke: "{stroke}", stroke_width: "2",
                       stroke_linejoin: "round", stroke_linecap: "round" }
                // Handles.
                circle {
                    cx: "{xa}", cy: "{ytop}", r: "5", fill: "#0e0e11", stroke: "{stroke}", stroke_width: "2",
                    style: "cursor: ew-resize;",
                    onpointerdown: move |_| grab.set(Some(Grab::Attack)),
                }
                circle {
                    cx: "{xd}", cy: "{sus_y}", r: "5", fill: "#0e0e11", stroke: "{stroke}", stroke_width: "2",
                    style: "cursor: move;",
                    onpointerdown: move |_| grab.set(Some(Grab::Decay)),
                }
                circle {
                    cx: "{xs}", cy: "{sus_y}", r: "4", fill: "{stroke}", fill_opacity: "0.5",
                    style: "cursor: ns-resize;",
                    onpointerdown: move |_| grab.set(Some(Grab::Sustain)),
                }
                circle {
                    cx: "{xr}", cy: "{y0}", r: "5", fill: "#0e0e11", stroke: "{stroke}", stroke_width: "2",
                    style: "cursor: ew-resize;",
                    onpointerdown: move |_| grab.set(Some(Grab::Release)),
                }
            }
            // Drag shield — maps pointer position back onto the grabbed value.
            if grab().is_some() {
                div {
                    style: "position: fixed; inset: 0; z-index: 999;",
                    onpointermove: move |e: PointerEvent| {
                        let el = e.element_coordinates();
                        let (px, py) = (el.x, el.y);
                        match grab() {
                            Some(Grab::Attack) => on_change.call(("attack", ms_for(px))),
                            Some(Grab::Decay) => {
                                on_change.call(("decay", ms_for(px - xa).max(0.0)));
                                let s = 1.0 - ((py - PAD) / (H - 2.0 * PAD)).clamp(0.0, 1.0);
                                on_change.call(("sustain", s as f32));
                            }
                            Some(Grab::Sustain) => {
                                let s = 1.0 - ((py - PAD) / (H - 2.0 * PAD)).clamp(0.0, 1.0);
                                on_change.call(("sustain", s as f32));
                            }
                            Some(Grab::Release) => on_change.call(("release", ms_for(px - xs).max(0.0))),
                            None => {}
                        }
                    },
                    onpointerup: move |_| grab.set(None),
                    onpointerleave: move |_| grab.set(None),
                }
            }
        }
    }
}

/// Filter response curve — a readable shape (not a sample-accurate response)
/// over a log-frequency axis, with the cutoff draggable horizontally and
/// resonance vertically.
#[component]
pub fn FilterCurve(
    cutoff_hz: f32,
    resonance: f32,
    #[props(default = "#a78bfa".to_string())] accent: String,
    #[props(default = true)] live: bool,
    /// `("cutoff" | "reso", value)`.
    on_change: EventHandler<(&'static str, f32)>,
) -> Element {
    let mut dragging = use_signal(|| false);
    let stroke = if live { accent.clone() } else { "#52525b".to_string() };

    // Log axis 20 Hz … 20 kHz. `fn` items (not closures) so the drag shield's
    // `move` handler doesn't try to borrow locals.
    fn x_of(f: f64) -> f64 {
        const LO: f64 = 2.995_732_273_553_991; // ln(20)
        const HI: f64 = 9.903_487_552_536_127; // ln(20_000)
        PAD + ((f.max(20.0).ln() - LO) / (HI - LO)) * (W - 2.0 * PAD)
    }
    fn f_of(x: f64) -> f64 {
        const LO: f64 = 2.995_732_273_553_991;
        const HI: f64 = 9.903_487_552_536_127;
        let t = ((x - PAD) / (W - 2.0 * PAD)).clamp(0.0, 1.0);
        (LO + t * (HI - LO)).exp()
    }
    let cutoff = cutoff_hz as f64;
    let res = resonance.clamp(0.0, 1.0) as f64;

    // A 2-pole lowpass magnitude, drawn across the axis with a resonant bump.
    let mut d = String::new();
    let steps = 80;
    for i in 0..=steps {
        let x = PAD + (i as f64 / steps as f64) * (W - 2.0 * PAD);
        let f = f_of(x);
        let r = f / cutoff.max(20.0);
        let mag = 1.0 / (1.0 + r.powi(4)).sqrt();
        let bump = res * 0.9 * (-((r.ln()).powi(2)) * 6.0).exp();
        let y = PAD + (1.0 - (mag + bump).clamp(0.0, 1.35) / 1.35) * (H - 2.0 * PAD);
        d.push_str(&format!("{} {x:.1} {y:.1}", if i == 0 { "M" } else { "L" }));
        d.push(' ');
    }
    let cx = x_of(cutoff);

    rsx! {
        div {
            style: "display: flex; flex-direction: column; gap: 6px; padding: 12px; \
                    border: 1px solid #1f1f23; border-radius: 14px; background: #0e0e11;",
            div { style: "display: flex; align-items: center; gap: 6px;",
                span { style: format!("width: 6px; height: 6px; border-radius: 999px; background: {stroke};") }
                span {
                    style: "font-size: 10px; font-weight: 700; letter-spacing: 0.08em; \
                            text-transform: uppercase; color: #a1a1aa;",
                    "Filter"
                }
                div { style: "flex: 1;" }
                span { style: "font-size: 9px; color: #52525b; font-variant-numeric: tabular-nums;",
                    {
                        if cutoff >= 1000.0 {
                            format!("{:.1} kHz · Q {:.2}", cutoff / 1000.0, resonance)
                        } else {
                            format!("{cutoff:.0} Hz · Q {resonance:.2}")
                        }
                    }
                }
            }
            svg {
                width: "{W}", height: "{H}", view_box: "0 0 {W} {H}",
                style: "touch-action: none; cursor: ew-resize;",
                onpointerdown: move |_| dragging.set(true),
                for frac in [0.25f64, 0.5, 0.75] {
                    line {
                        x1: "{PAD}", y1: "{PAD + frac * (H - 2.0 * PAD)}",
                        x2: "{W - PAD}", y2: "{PAD + frac * (H - 2.0 * PAD)}",
                        stroke: "#1b1b1f", stroke_width: "1",
                    }
                }
                path { d: "{d}", fill: "none", stroke: "{stroke}", stroke_width: "2", stroke_linejoin: "round" }
                line { x1: "{cx}", y1: "{PAD}", x2: "{cx}", y2: "{H - PAD}",
                       stroke: "{stroke}", stroke_width: "1", stroke_dasharray: "3 3", opacity: "0.5" }
                circle { cx: "{cx}", cy: "{PAD + 8.0}", r: "5", fill: "#0e0e11", stroke: "{stroke}", stroke_width: "2" }
            }
            if dragging() {
                div {
                    style: "position: fixed; inset: 0; z-index: 999;",
                    onpointermove: move |e: PointerEvent| {
                        let el = e.element_coordinates();
                        on_change.call(("cutoff", f_of(el.x) as f32));
                        let r = 1.0 - ((el.y - PAD) / (H - 2.0 * PAD)).clamp(0.0, 1.0);
                        on_change.call(("reso", r as f32));
                    },
                    onpointerup: move |_| dragging.set(false),
                    onpointerleave: move |_| dragging.set(false),
                }
            }
        }
    }
}
