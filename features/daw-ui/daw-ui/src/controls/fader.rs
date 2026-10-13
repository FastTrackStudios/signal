//! The volume fader, at any height, following the finger.
//!
//! Two problems that look unrelated and are both about the same thing —
//! the control being bigger than its art, and the control being ahead of
//! its engine.
//!
//! **Height.** The rail's art is 23x55 with the groove occupying rows
//! 16..39: fixed caps, a stretchy run between. Drawn as one `<svg>` scaled
//! to 300px the caps stretch too and the groove grows a solid bar at each
//! end. So it is drawn as [a stack of panes][daw_theme_art::slice::NamedArt::stack]
//! — fixed bands sized in pixels, the middle band `flex: 1` — and the
//! layout engine does the stretching. Nothing measures anything.
//!
//! The bare 14-ish pixels at each end are **correct**, and are what REAPER
//! renders: the cap art has no groove in it. If they look wrong in the app
//! the drawing is wrong, and the fix belongs in the source art's fixed
//! band, not in stretching the ends.
//!
//! **Latency.** The value under the pointer is the UI's, not the engine's —
//! see [`crate::controls::Drafts`] for why, and for the echo suppression
//! that stops the backend's own confirmation from fighting the drag.

use daw_theme_art::slice::{self, Pane};
use daw_theme_art::vector_controls as art;

use crate::controls::use_track_store;
use crate::core::sensitivity::DragSensitivity;
use crate::prelude::*;

/// A track's volume fader, wired to the DAW.
///
/// Fills the height it is given: put it in a flex column and let the strip
/// decide how tall the mixer is.
#[component]
pub fn VolumeFader(
    /// The track's GUID.
    track: String,
    /// Pixels per art pixel — sets the fader's *width* and the cap's size.
    /// The height comes from the parent box.
    #[props(default = 1.0)]
    scale: f32,
    /// The rail's height in px, when the caller knows it.
    ///
    /// The stretch band is `flex:1` with `height:100%`, and Blitz resolves
    /// that percentage against an auto height — so the band's *box* grew
    /// and the `<svg>` inside it stayed 23 rows tall, leaving the groove a
    /// short dash in the middle of a tall fader. Given the number, the
    /// bands are arithmetic instead: the caps keep their size and the run
    /// takes what is left.
    #[props(default)]
    height: Option<f32>,
) -> Element {
    let store = use_track_store();
    let mut drafts = store.drafts();

    let mut dragging = use_signal(|| false);
    let mut grab_y = use_signal(|| 0.0f32);
    let mut grab_value = use_signal(|| 0.0f64);

    // A drag is relative, not absolute: grabbing the cap must not jump the
    // value to wherever the pointer happened to land. `normal_pixels` is
    // how far the mouse travels for the full range, and shift is the fine
    // pass the rest of the crate's controls already answer to.
    let feel = DragSensitivity::DEFAULT;

    let rail = slice::expect_art("mcp_volbg");
    let cap = slice::expect_art("mcp_volthumb");
    let (rail_w, _) = rail.source;
    let (cap_w, cap_h) = cap.source;
    let w = (rail_w * scale).round() as u32;
    let cap_px = (
        (cap_w * scale).round() as u32,
        (cap_h * scale).round() as u32,
    );

    let guid = track.clone();
    let value = use_memo(use_reactive!(|guid| store.volume(&guid)));
    // The cap rides the rail with 0 at the bottom, and it is positioned by
    // its own top edge — so a cap at full is flush with the top and a cap
    // at zero is flush with the bottom, rather than half of it hanging off
    // each end.
    // Four places: enough that a pixel of travel is never lost on a tall
    // fader, few enough that the style string does not churn on float noise
    // every render.
    let travel = format!(
        "calc((100% - {}px) * {:.4})",
        cap_px.1,
        1.0 - position(value())
    );

    let guid = track.clone();
    let press = move |e: MouseEvent| {
        dragging.set(true);
        grab_y.set(e.client_coordinates().y as f32);
        grab_value.set(position(store.volume(&guid)));
    };
    let guid = track.clone();
    let drag = move |e: MouseEvent| {
        if !dragging() {
            return;
        }
        let dy = e.client_coordinates().y as f32 - grab_y();
        let mut span = feel.normal_pixels;
        if e.modifiers().shift() {
            span /= feel.fine_multiplier;
        }
        // Up is louder: screen y grows downward and the fader does not.
        let moved = -(dy / span) as f64;
        // The drag moves the cap, and the cap's position is what the taper
        // is expressed in — dragging the gain directly made the top half of
        // the rail cover 0 dB..+12 and the bottom half everything else.
        drafts.set_volume(&guid, gain((grab_value() + moved).clamp(0.0, 1.0)));
    };
    let guid = track.clone();
    let release = move |_| {
        if dragging.replace(false) {
            drafts.release_volume(&guid);
        }
    };

    rsx! {
        div {
            // `position: relative` so the cap can ride the rail, and an
            // explicit width because the art has one — only the height is
            // the parent's to decide.
            style: "position:relative; width:{w}px; height:100%; min-height:0; \
                    display:flex; flex-direction:column; user-select:none; cursor:ns-resize;",
            onmousedown: press,
            onmousemove: drag,
            onmouseup: release.clone(),
            // Leaving mid-drag ends it. Tracking the pointer outside the
            // element needs a window-level listener, which is a bigger
            // change than this control should carry alone.
            onmouseleave: release,

            // The rail, decomposed. One `<svg>` per band: the caps state
            // their height in pixels, the run takes what is left.
            for (i, pane) in rail.stack().into_iter().enumerate() {
                RailBand { key: "{i}", pane, width: w, height: band_px(pane, &rail, height, scale) }
            }

            // The cap, riding on top.
            div {
                style: "position:absolute; left:50%; top:{travel}; \
                        transform:translateX(-50%); line-height:0; pointer-events:none;",
                art::VolumeFaderCap { width: Some(cap_px.0), height: Some(cap_px.1) }
            }
        }
    }
}

/// One band of the rail.
///
/// A component rather than an inline call so each band is its own scope and
/// Dioxus diffs them independently — and so the `key` above means something.
#[component]
fn RailBand(pane: Pane, width: u32, height: Option<u32>) -> Element {
    rsx! {
        art::VolumeFaderTrack { pane: Some(pane), width: Some(width), height }
    }
}

/// A band's height in pixels, when the fader knows its own.
///
/// The fixed caps keep their source height; the growing run takes the
/// remainder. `None` when the caller did not say — the band then falls
/// back to the flex layout, which is right in a browser and short in
/// Blitz.
fn band_px(pane: Pane, rail: &slice::NamedArt, total: Option<f32>, scale: f32) -> Option<u32> {
    let total = total?;
    if !pane.grow {
        return Some((pane.view.3 * scale).round() as u32);
    }
    let fixed: f32 = rail
        .stack()
        .into_iter()
        .filter(|p| !p.grow)
        .map(|p| p.view.3 * scale)
        .sum();
    Some((total - fixed).max(1.0).round() as u32)
}

/// REAPER's fader taper, in both directions.
///
/// `Track::volume` is linear gain with 1.0 at 0 dB, and the rail's top is
/// **+12 dB**, not unity — so a fader that treated the gain as its position
/// pinned every default track to the very top of the rail, where REAPER puts
/// unity a little under three-quarters of the way up.
///
/// The curve is a fourth root over 0..+12 dB, which is the shape REAPER's own
/// fader has: it puts unity at 0.708 of travel against the 0.744 measured off
/// a screenshot of REAPER's mixer, and it is exact at both ends.
mod taper {
    /// +12 dB as gain — the top of the rail.
    pub const TOP: f64 = 3.981_071_705_534_972;
    pub const CURVE: f64 = 4.0;
}

/// Gain to cap position, 0 at the bottom of the rail and 1 at the top.
///
/// Public as `fader_position`: the track panel's volume knob has to swing
/// on the same taper the fader rides, or the two controls disagree about
/// where unity is.
pub fn fader_position(gain: f64) -> f64 {
    position(gain)
}

fn position(gain: f64) -> f64 {
    (gain.max(0.0) / taper::TOP)
        .powf(1.0 / taper::CURVE)
        .clamp(0.0, 1.0)
}

/// The inverse, for a drag — which moves the cap, not the gain.
fn gain(position: f64) -> f64 {
    taper::TOP * position.clamp(0.0, 1.0).powf(taper::CURVE)
}

#[cfg(test)]
mod taper_tests {
    use super::{gain, position, taper};

    /// Unity is where REAPER puts it — not at the top of the rail.
    #[test]
    fn unity_sits_below_the_top_of_the_rail() {
        let p = position(1.0);
        assert!((0.69..0.73).contains(&p), "unity at {p}, not ~0.71");
        assert_eq!(position(0.0), 0.0);
        assert!((position(taper::TOP) - 1.0).abs() < 1e-9);
    }

    /// And a drag round-trips: the position a gain shows at is the position
    /// that produces it again.
    #[test]
    fn the_taper_round_trips() {
        for g in [0.0, 0.25, 0.5, 1.0, 2.0, taper::TOP] {
            let back = gain(position(g));
            assert!((back - g).abs() < 1e-9, "{g} came back as {back}");
        }
    }
}
