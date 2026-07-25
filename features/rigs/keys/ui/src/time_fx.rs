//! **The time-domain picture** — one delay view and one reverb view, drawing
//! every loaded lane at once.
//!
//! The guitar rig draws two delays and two reverbs, a lane each, because it
//! has two of each block. A keys rig has one delay and one reverb *per lane*,
//! and six engines' worth of lanes — so stacking them would be a wall of
//! sixteen-pixel strips. Instead each view is one full-height set of axes with
//! every lane's tail overlaid on it, coloured by the engine it belongs to:
//! the Keys piano's repeats are green, the Pad's are blue, and you can see at
//! a glance that the pad's tail is what is washing over the downbeat.
//!
//! The selection decides **focus**, not membership: the lanes the knobs reach
//! draw solid and labelled, everything else stays faint behind them. Nothing
//! disappears when you pick a lane — the point of the view is the relationship
//! between the tails.
//!
//! Maths follows the guitar rig's panels (`signal-guitar-ui`): a delay is a
//! decaying tap train (`amp *= feedback` every `time`), a reverb is a
//! dB-linear tail to its RT60 on a log time axis.

use dioxus::prelude::*;

/// One lane's time-domain settings, as the mixer reports them.
#[derive(Clone, PartialEq, Debug)]
pub struct FxLane {
    /// Lane name ("Keys A") — the label on a focused tail.
    pub label: String,
    /// The engine's colour: this is how a tail is identified.
    pub color: String,
    /// Whether the current selection reaches this lane.
    pub focus: bool,
    /// Delay: time between repeats, how much survives each one, how much of
    /// it is heard.
    pub delay_ms: f32,
    pub feedback: f32,
    pub delay_mix: f32,
    /// Reverb: how long the tail runs, how big the space is, how much of it
    /// is heard, and how late it starts.
    pub decay: f32,
    pub size: f32,
    pub verb_mix: f32,
    pub predelay_ms: f32,
}

impl FxLane {
    /// Nothing audible to draw.
    fn silent_delay(&self) -> bool {
        self.delay_mix <= 0.001 || self.delay_ms <= 0.0
    }

    fn silent_verb(&self) -> bool {
        self.verb_mix <= 0.001
    }
}

/// Decay knob (0..1) → RT60 seconds. Same curve as the guitar rig's: musical
/// at the bottom of the range, cathedral at the top.
fn t60_secs(decay: f32, size: f32) -> f64 {
    let base = 0.2 + (decay.clamp(0.0, 1.0) as f64).powf(1.8) * 11.0;
    base * (0.6 + 0.8 * size.clamp(0.0, 1.0) as f64)
}

const W: f64 = 1000.0;
const H: f64 = 150.0;
const PAD: f64 = 8.0;

/// The stroke/fill weights for a lane: focused lanes are the subject, the
/// rest are context.
fn weights(focus: bool) -> (&'static str, &'static str, &'static str) {
    if focus {
        ("0.95", "0.18", "2")
    } else {
        ("0.25", "0.05", "1")
    }
}

/// **Delay** — every lane's repeats on one time ruler, in beats.
///
/// The ruler is beats rather than seconds because that is how a delay is set
/// against a song: a lane whose repeats land on the off-beat is visible as
/// taps that sit between the gridlines.
#[component]
pub fn DelayView(lanes: Vec<FxLane>, #[props(default = 120.0)] tempo_bpm: f32) -> Element {
    let quarter_ms = 60_000.0 / tempo_bpm.max(20.0);
    // Eight beats of window: two bars of 4/4, enough to see a dotted-eighth
    // pattern resolve without squashing a slow half-note delay.
    let win_ms = quarter_ms * 8.0;
    let x_of = |ms: f32| PAD + (ms as f64 / win_ms as f64).clamp(0.0, 1.0) * (W - 2.0 * PAD);
    let audible: Vec<&FxLane> = lanes.iter().filter(|l| !l.silent_delay()).collect();

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 4px; min-width: 0;",
            div { style: "display: flex; align-items: baseline; gap: 8px;",
                span {
                    style: "font-size: 9px; font-weight: 700; letter-spacing: 0.1em; \
                            text-transform: uppercase; color: #71717a;",
                    "Delay"
                }
                span { style: "font-size: 8px; color: #3f3f46;",
                    {format!("{:.0} bpm · 8 beats", tempo_bpm)}
                }
                div { style: "flex: 1;" }
                if audible.is_empty() {
                    span { style: "font-size: 8px; color: #3f3f46;", "no lane is sending to a delay" }
                }
            }
            svg {
                width: "100%", height: "100%", view_box: "0 0 {W} {H}",
                preserve_aspect_ratio: "none",
                style: "display: block; width: 100%; min-height: 0;",
                // Beat grid — bar lines brighter than beats.
                for beat in 0..=8 {
                    line {
                        key: "beat-{beat}",
                        x1: "{x_of(beat as f32 * quarter_ms):.1}", y1: "{PAD}",
                        x2: "{x_of(beat as f32 * quarter_ms):.1}", y2: "{H - PAD}",
                        stroke: "#ffffff",
                        stroke_opacity: if beat % 4 == 0 { "0.10" } else { "0.04" },
                        stroke_width: "1",
                    }
                }
                line {
                    x1: "{PAD}", y1: "{H - PAD}", x2: "{W - PAD}", y2: "{H - PAD}",
                    stroke: "#27272a", stroke_width: "1",
                }
                // The dry hit every tail decays from.
                rect {
                    x: "{PAD - 1.0}", y: "{PAD}", width: "3", height: "{H - 2.0 * PAD}",
                    fill: "#e4e4e7", fill_opacity: "0.5", rx: "1",
                }
                for (li, lane) in audible.iter().enumerate() {
                    {
                        let (stroke_op, _fill_op, w) = weights(lane.focus);
                        let fb = lane.feedback.clamp(0.0, 0.95);
                        let floor = H - PAD;
                        let span = H - 2.0 * PAD;
                        // The tap train: each repeat keeps `fb` of the last.
                        let mut taps: Vec<(f64, f64)> = Vec::new();
                        let (mut amp, mut t) = (lane.delay_mix.clamp(0.0, 1.0) as f64, lane.delay_ms);
                        while (t as f64) <= win_ms as f64 && amp > 0.01 && taps.len() < 48 {
                            taps.push((x_of(t), amp));
                            amp *= fb as f64;
                            t += lane.delay_ms;
                        }
                        // A line through the tap peaks — the decay envelope,
                        // which is what tells two lanes apart at a glance.
                        let curve = taps
                            .iter()
                            .map(|(x, a)| format!("{x:.1} {:.1}", floor - a * span))
                            .collect::<Vec<_>>()
                            .join(" L ");
                        let curve = if curve.is_empty() {
                            String::new()
                        } else {
                            format!("M {:.1} {:.1} L {curve}", x_of(0.0), floor - (lane.delay_mix as f64) * span)
                        };
                        rsx! {
                            g { key: "dly-{li}",
                                if !curve.is_empty() {
                                    path {
                                        d: "{curve}", fill: "none", stroke: "{lane.color}",
                                        stroke_opacity: "{stroke_op}", stroke_width: "{w}",
                                        stroke_linejoin: "round", stroke_dasharray: "3 3",
                                    }
                                }
                                for (i, (x, a)) in taps.iter().enumerate() {
                                    rect {
                                        key: "{i}",
                                        x: "{x - 1.5:.1}", y: "{floor - a * span:.1}",
                                        width: "3", height: "{a * span:.1}",
                                        fill: "{lane.color}", fill_opacity: "{stroke_op}", rx: "1",
                                    }
                                }
                                if lane.focus {
                                    if let Some((x, a)) = taps.first() {
                                        text {
                                            x: "{x + 5.0:.1}", y: "{floor - a * span - 4.0:.1}",
                                            fill: "{lane.color}", font_size: "9", font_weight: "700",
                                            "{lane.label}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// **Reverb** — every lane's tail on one log time axis (0.1–20 s).
#[component]
pub fn ReverbView(lanes: Vec<FxLane>) -> Element {
    let x_of = |t: f64| -> f64 {
        let (lo, hi) = (0.1f64, 20.0f64);
        PAD + ((t.max(lo) / lo).ln() / (hi / lo).ln()).clamp(0.0, 1.0) * (W - 2.0 * PAD)
    };
    let audible: Vec<&FxLane> = lanes.iter().filter(|l| !l.silent_verb()).collect();
    const MARKS: &[(f64, &str)] =
        &[(0.25, "¼s"), (0.5, "½s"), (1.0, "1s"), (2.0, "2s"), (4.0, "4s"), (8.0, "8s"), (16.0, "16s")];

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 4px; min-width: 0;",
            div { style: "display: flex; align-items: baseline; gap: 8px;",
                span {
                    style: "font-size: 9px; font-weight: 700; letter-spacing: 0.1em; \
                            text-transform: uppercase; color: #71717a;",
                    "Reverb"
                }
                span { style: "font-size: 8px; color: #3f3f46;", "0.1 – 20 s" }
                div { style: "flex: 1;" }
                if audible.is_empty() {
                    span { style: "font-size: 8px; color: #3f3f46;", "no lane is sending to a reverb" }
                }
            }
            svg {
                width: "100%", height: "100%", view_box: "0 0 {W} {H}",
                preserve_aspect_ratio: "none",
                style: "display: block; width: 100%; min-height: 0;",
                for (i, (t, label)) in MARKS.iter().enumerate() {
                    g { key: "mark-{i}",
                        line {
                            x1: "{x_of(*t):.1}", y1: "{PAD}", x2: "{x_of(*t):.1}", y2: "{H - PAD}",
                            stroke: "#ffffff", stroke_opacity: "0.05", stroke_width: "1",
                        }
                        text {
                            x: "{x_of(*t) + 3.0:.1}", y: "{H - PAD - 3.0:.1}",
                            fill: "#3f3f46", font_size: "8",
                            "{label}"
                        }
                    }
                }
                line {
                    x1: "{PAD}", y1: "{H - PAD}", x2: "{W - PAD}", y2: "{H - PAD}",
                    stroke: "#27272a", stroke_width: "1",
                }
                for (li, lane) in audible.iter().enumerate() {
                    {
                        let (stroke_op, fill_op, w) = weights(lane.focus);
                        let t60 = t60_secs(lane.decay, lane.size);
                        let floor = H - PAD;
                        let span = H - 2.0 * PAD;
                        let mix = lane.verb_mix.clamp(0.0, 1.0) as f64;
                        // The tail starts after its pre-delay and falls
                        // dB-linearly to nothing at t60.
                        let pre = (lane.predelay_ms.max(0.0) as f64) / 1000.0;
                        let start = x_of(pre.max(0.02));
                        let mut d = format!("M {start:.1} {floor:.1} ");
                        for px in 0..=110 {
                            let frac = px as f64 / 110.0;
                            let t = 0.1 * (20.0f64 / 0.1).powf(frac);
                            if t < pre {
                                continue;
                            }
                            let a = (1.0 - (t - pre) / t60).max(0.0);
                            d.push_str(&format!("L {:.1} {:.1} ", x_of(t), floor - a * mix * span));
                        }
                        d.push_str(&format!("L {:.1} {floor:.1} Z", x_of(20.0)));
                        rsx! {
                            g { key: "verb-{li}",
                                path {
                                    d: "{d}", fill: "{lane.color}", fill_opacity: "{fill_op}",
                                    stroke: "{lane.color}", stroke_opacity: "{stroke_op}",
                                    stroke_width: "{w}", stroke_linejoin: "round",
                                }
                                // Where this lane's tail dies.
                                line {
                                    x1: "{x_of(pre + t60):.1}", y1: "{PAD + 4.0}",
                                    x2: "{x_of(pre + t60):.1}", y2: "{floor}",
                                    stroke: "{lane.color}", stroke_opacity: "{stroke_op}",
                                    stroke_width: "1", stroke_dasharray: "2 3",
                                }
                                if lane.focus {
                                    text {
                                        x: "{x_of(pre + t60) - 4.0:.1}", y: "{PAD + 12.0}",
                                        fill: "{lane.color}", font_size: "9", font_weight: "700",
                                        text_anchor: "end",
                                        {format!("{} · {:.1}s", lane.label, t60)}
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
