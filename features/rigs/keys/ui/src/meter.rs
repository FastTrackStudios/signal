//! Level meters — **the edge of the thing that is sounding**.
//!
//! The rig's console idiom is that a level lives on an edge rather than in a
//! row: an engine's fader IS its card's left edge ([`EdgeFader`](crate::fader::EdgeFader)).
//! A meter is the same move on the opposite edge — what actually came out,
//! along the bottom of the card, and along the bottom of a lane's letter.
//! Nothing new to look at, no row of numbers: the shape you already reach for
//! lights up when it is making sound.
//!
//! The peaks are real audio: every Engine/Layer/module container in the render
//! tree carries a post-fader peak cell written on the audio thread (with its
//! own fall-back), published in `KeysStatus.meters` at the rig's 30 Hz pump.

use dioxus::prelude::*;

/// The meter's floor, in dBFS. Full width is 0 dBFS — the meter is headroom,
/// not fader travel, so it reads clip at the right edge.
const FLOOR_DB: f32 = -60.0;

/// Linear peak → 0..1 across the meter's scale. Same 0.6 taper the fader
/// uses, so the two read alike: the top of the range is where the decisions
/// are, the bottom compresses into the floor.
pub fn peak_pos(peak: f32) -> f32 {
    if peak <= 0.0 {
        return 0.0;
    }
    let db = 20.0 * peak.max(1e-6).log10();
    let t = ((db - FLOOR_DB) / -FLOOR_DB).clamp(0.0, 1.0);
    t.powf(0.6)
}

/// The colour of a peak: the engine's accent while there is headroom, amber
/// as it runs out, red into the last dB. A summed multi-layer patch clips
/// long before any one lane looks loud — that is what this is for.
fn peak_color(peak: f32, accent: &str) -> String {
    let db = if peak <= 0.0 {
        f32::NEG_INFINITY
    } else {
        20.0 * peak.log10()
    };
    if db >= -1.0 {
        "#f87171".to_string()
    } else if db >= -6.0 {
        "#fbbf24".to_string()
    } else {
        accent.to_string()
    }
}

/// **A card's bottom edge, as a meter.** Absolutely positioned, so it fills
/// whatever it is dropped into and takes that shape's corner radius — the
/// meter is the outline, not a widget sitting inside it.
///
/// The parent must be `position: relative`.
#[component]
pub fn EdgeMeter(
    /// Post-fader peak, linear (0..~1).
    peak: f32,
    /// Bar thickness.
    #[props(default = 4)]
    height_px: u32,
    /// Matches the host's corner radius so the edge stays the outline.
    #[props(default = 12)]
    radius_px: u32,
    #[props(default = "#38bdf8".to_string())] accent: String,
    /// Muted lane / bypassed engine — it still meters (a muted lane's meter
    /// sitting dark is the answer to "why can't I hear it"), just quietly.
    #[props(default = false)]
    dimmed: bool,
    /// Left inset, for a card whose left edge is already a fader — the meter
    /// starts where the fader ends, and gives up its rounded left corner.
    #[props(default = 0)]
    left_px: u32,
) -> Element {
    let pct = peak_pos(peak) * 100.0;
    let color = peak_color(peak, &accent);
    let opacity = if dimmed { "0.35" } else { "1" };
    let inner_radius = radius_px.saturating_sub(1);
    let left_radius = if left_px == 0 { radius_px } else { 0 };

    rsx! {
        div {
            style: "position: absolute; left: {left_px}px; right: 0; bottom: 0; height: {height_px}px; \
                    border-radius: 0 0 {radius_px}px {left_radius}px; background: #131316; \
                    overflow: hidden; opacity: {opacity}; pointer-events: none;",
            title: "{fmt_dbfs(peak)} dBFS",
            div {
                style: "position: absolute; left: 0; top: 0; bottom: 0; width: {pct:.1}%; \
                        border-radius: 0 0 {inner_radius}px {inner_radius}px; \
                        background: linear-gradient(90deg, {accent}, {color});",
            }
            // Where the last dB of headroom starts — the mark you want the
            // peaks to stay left of.
            {
                let hot = peak_pos(10f32.powf(-6.0 / 20.0)) * 100.0;
                rsx! {
                    div {
                        style: "position: absolute; top: 0; bottom: 0; left: {hot:.1}%; \
                                width: 1px; background: #3f3f46;",
                    }
                }
            }
        }
    }
}

/// A peak as a console reads it: dBFS, or `−∞` for silence.
pub fn fmt_dbfs(peak: f32) -> String {
    if peak <= 0.0 {
        "−∞".into()
    } else {
        format!("{:.1}", 20.0 * peak.log10())
    }
}

/// The live peak of one metered container (engine / layer / module), by the
/// same name its fader is addressed with. `0.0` when the rig isn't running or
/// the host provided no state context.
pub fn use_peak(name: &str) -> f32 {
    let state = use_hook(try_consume_context::<crate::state::KeysViewState>);
    let Some(state) = state else { return 0.0 };
    let status = state.status.read();
    status
        .meters
        .iter()
        .find(|m| m.name == name)
        .map(|m| m.peak)
        .unwrap_or(0.0)
}
