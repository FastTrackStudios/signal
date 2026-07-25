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

const W: f64 = 600.0;
const H: f64 = 190.0;
const PAD: f64 = 6.0;

/// A graph's own card chrome. `flat: true` drops it — the graph is sitting
/// inside a `Panel` that already draws the box and owns the title, and two
/// nested cards read as noise rather than structure.
fn chrome(flat: bool, gap_px: u32) -> String {
    if flat {
        format!("display: flex; flex-direction: column; gap: {gap_px}px;")
    } else {
        format!(
            "display: flex; flex-direction: column; gap: {gap_px}px; padding: 14px; \
             border: 1px solid #1f1f23; border-radius: 14px; background: #0e0e11;"
        )
    }
}
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
    /// Drop the card chrome — the graph is inside a `Panel`.
    #[props(default = false)] flat: bool,
    /// `(macro-id-suffix, value)` — "attack" | "decay" | "sustain" | "release".
    on_change: EventHandler<(&'static str, f32)>,
) -> Element {
    let mut grab = use_signal(|| None::<Grab>);
    // (pointer x, pointer y, values at grab) while a drag is live. Deltas —
    // not absolute positions — so a stretched SVG still tracks the pointer.
    let mut drag = use_signal(|| None::<(f64, f64, Adsr)>);

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
    let box_style = chrome(flat, 10);

    rsx! {
        div {
            style: "{box_style}",
            div { style: "display: flex; align-items: center; gap: 8px;",
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
                // Stretches to the panel: the viewBox is the drawing space,
                // CSS decides the size. Drags are delta-based, so the scale
                // factor never has to be reconstructed.
                width: "100%", height: "{H}", view_box: "0 0 {W} {H}",
                preserve_aspect_ratio: "none",
                style: "touch-action: none; cursor: crosshair; display: block; width: 100%;",
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
                    onpointerdown: move |e: PointerEvent| {
                        grab.set(Some(Grab::Attack));
                        drag.set(Some((e.client_coordinates().x, e.client_coordinates().y, adsr)));
                    },
                }
                circle {
                    cx: "{xd}", cy: "{sus_y}", r: "5", fill: "#0e0e11", stroke: "{stroke}", stroke_width: "2",
                    style: "cursor: move;",
                    onpointerdown: move |e: PointerEvent| {
                        grab.set(Some(Grab::Decay));
                        drag.set(Some((e.client_coordinates().x, e.client_coordinates().y, adsr)));
                    },
                }
                circle {
                    cx: "{xs}", cy: "{sus_y}", r: "4", fill: "{stroke}", fill_opacity: "0.5",
                    style: "cursor: ns-resize;",
                    onpointerdown: move |e: PointerEvent| {
                        grab.set(Some(Grab::Sustain));
                        drag.set(Some((e.client_coordinates().x, e.client_coordinates().y, adsr)));
                    },
                }
                circle {
                    cx: "{xr}", cy: "{y0}", r: "5", fill: "#0e0e11", stroke: "{stroke}", stroke_width: "2",
                    style: "cursor: ew-resize;",
                    onpointerdown: move |e: PointerEvent| {
                        grab.set(Some(Grab::Release));
                        drag.set(Some((e.client_coordinates().x, e.client_coordinates().y, adsr)));
                    },
                }
            }
            // Drag shield: pointer deltas → value changes. 400 px of travel
            // sweeps a segment's full range; vertical drag sets sustain.
            if grab().is_some() {
                div {
                    style: "position: fixed; inset: 0; z-index: 999;",
                    onpointermove: move |e: PointerEvent| {
                        let Some((x0, y0, start)) = drag() else { return };
                        let dx = (e.client_coordinates().x - x0) as f32;
                        let dy = (e.client_coordinates().y - y0) as f32;
                        let time = |base: f32, max: f32| (base + dx / 400.0 * max).clamp(0.0, max);
                        let level = |base: f32| (base - dy / 200.0).clamp(0.0, 1.0);
                        match grab() {
                            Some(Grab::Attack) => on_change.call(("attack", time(start.attack_ms, 5000.0))),
                            Some(Grab::Decay) => {
                                on_change.call(("decay", time(start.decay_ms, 5000.0)));
                                on_change.call(("sustain", level(start.sustain)));
                            }
                            Some(Grab::Sustain) => on_change.call(("sustain", level(start.sustain))),
                            Some(Grab::Release) => {
                                on_change.call(("release", time(start.release_ms, 8000.0)))
                            }
                            None => {}
                        }
                    },
                    onpointerup: move |_| { grab.set(None); drag.set(None); },
                    onpointerleave: move |_| { grab.set(None); drag.set(None); },
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
    /// Drop the card chrome and the duplicated title — the graph is inside a
    /// `Panel` that already says "Filter"; only the readout is kept.
    #[props(default = false)] flat: bool,
    /// `("cutoff" | "reso", value)`.
    on_change: EventHandler<(&'static str, f32)>,
) -> Element {
    // (pointer x, pointer y, cutoff, resonance) at grab.
    let mut dragging = use_signal(|| None::<(f64, f64)>);
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
    let box_style = chrome(flat, 10);

    rsx! {
        div {
            style: "{box_style}",
            div { style: "display: flex; align-items: center; gap: 8px;",
                if !flat {
                    span { style: format!("width: 6px; height: 6px; border-radius: 999px; background: {stroke};") }
                    span {
                        style: "font-size: 10px; font-weight: 700; letter-spacing: 0.08em; \
                                text-transform: uppercase; color: #a1a1aa;",
                        "Filter"
                    }
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
                width: "100%", height: "{H}", view_box: "0 0 {W} {H}",
                preserve_aspect_ratio: "none",
                style: "touch-action: none; cursor: ew-resize; display: block; width: 100%;",
                onpointerdown: move |e: PointerEvent| {
                    dragging.set(Some((e.client_coordinates().x, e.client_coordinates().y)));
                },
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
            if dragging().is_some() {
                div {
                    style: "position: fixed; inset: 0; z-index: 999;",
                    onpointermove: move |e: PointerEvent| {
                        let Some((x0, y0)) = dragging() else { return };
                        let dx = e.client_coordinates().x - x0;
                        let dy = e.client_coordinates().y - y0;
                        // Horizontal = cutoff on a log axis (600 px ≈ the
                        // whole 20 Hz … 20 kHz sweep); vertical = resonance.
                        let octaves = dx / 600.0 * 10.0;
                        let next = (cutoff * 2f64.powf(octaves)).clamp(20.0, 20_000.0);
                        on_change.call(("cutoff", next as f32));
                        let r = (res - dy / 200.0).clamp(0.0, 1.0);
                        on_change.call(("reso", r as f32));
                        dragging.set(Some((e.client_coordinates().x, e.client_coordinates().y)));
                    },
                    onpointerup: move |_| dragging.set(None),
                    onpointerleave: move |_| dragging.set(None),
                }
            }
        }
    }
}

// ── Layer overlays ──────────────────────────────────────────────────────────
//
// The layer view draws every module on one pair of axes: envelopes together,
// filters together, a colour per module. That is the whole point of the layer
// macros — you turn one knob and watch the shapes move *relative to each
// other*, which is invisible when each module is a separate page.

/// A module's colour on the layer overlays. Stable by slot index, so module A
/// is the same colour everywhere it appears.
pub fn module_color(index: u32) -> &'static str {
    const PALETTE: &[&str] = &["#38bdf8", "#f472b6", "#fbbf24", "#4ade80", "#a78bfa", "#fb7185"];
    PALETTE[(index as usize) % PALETTE.len()]
}

/// One module's contribution to an overlay.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct ModuleCurve {
    /// Slot label ("A") — the legend.
    pub slot: String,
    pub color: String,
    /// Amp envelope, filter envelope.
    pub amp: Adsr,
    pub filter: Adsr,
    /// Filter response.
    pub cutoff_hz: f32,
    pub resonance: f32,
    /// Silent modules draw faintly instead of vanishing.
    pub dim: bool,
    /// Inside what the knobs above are driving. The mixer's band draws
    /// **every** sound in the rig on one set of axes, so the ones the current
    /// selection reaches are drawn at full strength and the rest sit behind
    /// them — you see what you are about to move, in the company of what you
    /// are not.
    pub focus: bool,
}

/// How strongly a curve is drawn: `(stroke opacity, stroke width, fill
/// opacity)`. Four states, in falling order of "does this concern me".
fn ink(c: &ModuleCurve) -> (&'static str, &'static str, &'static str) {
    match (c.focus, c.dim) {
        (true, false) => ("1", "2.25", "0.07"),
        (true, true) => ("0.4", "1.5", "0.03"),
        (false, false) => ("0.4", "1.5", "0.03"),
        (false, true) => ("0.16", "1", "0"),
    }
}

/// The ADSR path for an overlay (same geometry as [`EnvelopeGraph`], scaled to
/// `h`).
fn env_path(a: &Adsr, h: f64) -> String {
    let xa = x_for(a.attack_ms as f64);
    let xd = x_for((a.attack_ms + a.decay_ms) as f64);
    let sus_y = PAD + (1.0 - a.sustain.clamp(0.0, 1.0) as f64) * (h - 2.0 * PAD);
    let xs = (xd + 34.0).min(W - PAD - 20.0);
    let xr = (xs + x_for(a.release_ms as f64) - PAD).min(W - PAD);
    let (y0, ytop) = (h - PAD, PAD);
    format!("M {PAD:.1} {y0:.1} L {xa:.1} {ytop:.1} L {xd:.1} {sus_y:.1} L {xs:.1} {sus_y:.1} L {xr:.1} {y0:.1}")
}

/// Legend chips — module slot in its colour.
#[component]
fn Legend(curves: Vec<ModuleCurve>, note: String) -> Element {
    rsx! {
        div { style: "display: flex; align-items: center; gap: 10px; flex-wrap: wrap;",
            for c in curves.iter() {
                div {
                    key: "{c.slot}",
                    style: "display: flex; align-items: center; gap: 4px;",
                    span {
                        style: format!(
                            "width: 10px; height: 3px; border-radius: 2px; background: {}; opacity: {};",
                            c.color,
                            ink(c).0,
                        ),
                    }
                    span {
                        style: format!(
                            "font-size: 9px; font-weight: {}; color: {};",
                            if c.focus { "700" } else { "600" },
                            if c.focus && !c.dim { "#a1a1aa" } else { "#52525b" },
                        ),
                        "{c.slot}"
                    }
                }
            }
            div { style: "flex: 1;" }
            span { style: "font-size: 9px; color: #3f3f46;", "{note}" }
        }
    }
}

/// **Amp + Filter envelopes of every module, on one set of axes.** Amp is
/// solid, filter dashed; a layer macro moves them all and the relationship
/// between them stays visible.
#[component]
pub fn StackedEnvelopes(
    curves: Vec<ModuleCurve>,
    #[props(default = 260)] height_px: u32,
    /// `true` draws the amp envelopes, `false` the filter envelopes. The
    /// Filter card wants only its own; a combined view passes both.
    #[props(default = true)] amp: bool,
    #[props(default = false)] both: bool,
    /// Drop the card chrome and header — the caption above owns the title.
    #[props(default = false)] flat: bool,
) -> Element {
    let h = height_px as f64;
    let box_style = chrome(flat, 10);
    rsx! {
        div {
            style: "{box_style}",
            if !flat {
                div { style: "display: flex; align-items: center; gap: 8px;",
                    span {
                        style: "font-size: 11px; font-weight: 700; letter-spacing: 0.08em; \
                                text-transform: uppercase; color: #d4d4d8;",
                        if both { "Envelopes" } else if amp { "Amp Envelope" } else { "Filter Envelope" }
                    }
                    span { style: "font-size: 9px; color: #52525b;",
                        if both { "amp solid · filter dashed" } else { "one curve per module" }
                    }
                }
            }
            svg {
                width: "100%", height: "{h}", view_box: "0 0 {W} {h}",
                preserve_aspect_ratio: "none",
                style: "display: block; width: 100%;",
                for frac in [0.25f64, 0.5, 0.75] {
                    line {
                        x1: "{PAD}", y1: "{PAD + frac * (h - 2.0 * PAD)}",
                        x2: "{W - PAD}", y2: "{PAD + frac * (h - 2.0 * PAD)}",
                        stroke: "#1b1b1f", stroke_width: "1",
                    }
                }
                for c in curves.iter() {
                    {
                        let solid = env_path(if both || amp { &c.amp } else { &c.filter }, h);
                        let dashed = both.then(|| env_path(&c.filter, h));
                        let (op, width, fill) = ink(c);
                        rsx! {
                            g { key: "{c.slot}",
                                path {
                                    d: "{solid}", fill: "{c.color}", fill_opacity: "{fill}",
                                    stroke: "{c.color}", stroke_width: "{width}", stroke_opacity: "{op}",
                                    stroke_linejoin: "round", stroke_linecap: "round",
                                }
                                if let Some(d) = dashed {
                                    path {
                                        d: "{d}", fill: "none", stroke: "{c.color}",
                                        stroke_width: "1.5", stroke_opacity: "{op}",
                                        stroke_dasharray: "6 4", stroke_linejoin: "round",
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Legend { curves: curves.clone(), note: "0 – 4 s".to_string() }
        }
    }
}

/// **Every module's filter response on one axis** — see the layer's cutoffs
/// move together (and keep their spacing) as the Global Control turns.
#[component]
pub fn StackedFilters(
    curves: Vec<ModuleCurve>,
    #[props(default = 260)] height_px: u32,
    /// Drop the card chrome and header — the `Panel` owns the title.
    #[props(default = false)] flat: bool,
) -> Element {
    let h = height_px as f64;
    let box_style = chrome(flat, 10);
    fn x_of(f: f64) -> f64 {
        const LO: f64 = 2.995_732_273_553_991;
        const HI: f64 = 9.903_487_552_536_127;
        PAD + ((f.max(20.0).ln() - LO) / (HI - LO)) * (W - 2.0 * PAD)
    }
    fn f_of(x: f64) -> f64 {
        const LO: f64 = 2.995_732_273_553_991;
        const HI: f64 = 9.903_487_552_536_127;
        let t = ((x - PAD) / (W - 2.0 * PAD)).clamp(0.0, 1.0);
        (LO + t * (HI - LO)).exp()
    }
    rsx! {
        div {
            style: "{box_style}",
            if !flat {
                div { style: "display: flex; align-items: center; gap: 8px;",
                    span {
                        style: "font-size: 11px; font-weight: 700; letter-spacing: 0.08em; \
                                text-transform: uppercase; color: #d4d4d8;",
                        "Filter"
                    }
                    span { style: "font-size: 9px; color: #52525b;", "20 Hz – 20 kHz" }
                }
            }
            svg {
                width: "100%", height: "{h}", view_box: "0 0 {W} {h}",
                preserve_aspect_ratio: "none",
                style: "display: block; width: 100%;",
                // Decade grid.
                for f in [100.0f64, 1000.0, 10000.0] {
                    line {
                        x1: "{x_of(f)}", y1: "{PAD}", x2: "{x_of(f)}", y2: "{h - PAD}",
                        stroke: "#1b1b1f", stroke_width: "1",
                    }
                }
                for c in curves.iter() {
                    {
                        let cutoff = c.cutoff_hz as f64;
                        let res = c.resonance.clamp(0.0, 1.0) as f64;
                        let mut d = String::new();
                        let steps = 90;
                        for i in 0..=steps {
                            let x = PAD + (i as f64 / steps as f64) * (W - 2.0 * PAD);
                            let r = f_of(x) / cutoff.max(20.0);
                            let mag = 1.0 / (1.0 + r.powi(4)).sqrt();
                            let bump = res * 0.9 * (-((r.ln()).powi(2)) * 6.0).exp();
                            let y = PAD + (1.0 - (mag + bump).clamp(0.0, 1.35) / 1.35) * (h - 2.0 * PAD);
                            d.push_str(&format!("{} {x:.1} {y:.1} ", if i == 0 { "M" } else { "L" }));
                        }
                        let cx = x_of(cutoff);
                        let (op, width, _) = ink(c);
                        rsx! {
                            g { key: "{c.slot}",
                                path {
                                    d: "{d}", fill: "none", stroke: "{c.color}", stroke_width: "{width}",
                                    stroke_opacity: "{op}", stroke_linejoin: "round",
                                }
                                line {
                                    x1: "{cx}", y1: "{PAD}", x2: "{cx}", y2: "{h - PAD}",
                                    stroke: "{c.color}", stroke_width: "1",
                                    stroke_opacity: if c.focus { "0.35" } else { "0.15" },
                                    stroke_dasharray: "3 4",
                                }
                            }
                        }
                    }
                }
            }
            Legend { curves: curves.clone(), note: "cutoff marked per module".to_string() }
        }
    }
}
