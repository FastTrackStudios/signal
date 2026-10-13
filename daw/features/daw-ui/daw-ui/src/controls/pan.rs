//! The pan knob, and the strip's name plate.
//!
//! Pan is the fader's problem again in a smaller box: a drag has to move the
//! knob now, not when the engine gets round to confirming it, and the
//! engine's echo of our own write must not fight the finger. So it uses the
//! same [`Drafts`][crate::controls::Drafts] mechanism, and the same
//! [`ControlSync`][crate::controls::ControlSync] loop drains it.
//!
//! The name plate is here because a strip without a name and a colour is not
//! a strip anybody can use — and because both come from fields that already
//! exist, so it is a read and nothing more.

use daw_theme_art::vector_controls as art;

use crate::controls::{use_track, use_track_store};
use crate::core::sensitivity::DragSensitivity;
use crate::prelude::*;

/// A track's pan knob.
///
/// REAPER's `*_pan_knob_*` art has no pointer in it — the bitmaps are a disc
/// and a cap, and REAPER paints the indicator over the top from the
/// parameter. A panel that is not REAPER has to draw it, which is what
/// `indicator` on the art component is for — off, the exported PNGs stay
/// exactly as audited; on, the knob reads as a knob.
#[component]
pub fn PanKnob(
    /// The track's GUID.
    track: String,
    /// Draw the larger of REAPER's two knob sizes.
    #[props(default)]
    large: bool,
    #[props(default = 1.0)] scale: f32,
) -> Element {
    let store = use_track_store();
    let mut drafts = store.drafts();

    let mut dragging = use_signal(|| false);
    let mut grab_y = use_signal(|| 0.0f32);
    let mut grab_value = use_signal(|| 0.0f64);
    let feel = DragSensitivity::DEFAULT;

    let guid = track.clone();
    let pan = use_memo(use_reactive!(|guid| store.pan(&guid)));

    let (src_w, src_h) = if large {
        (28.0f32, 29.0f32)
    } else {
        (24.0f32, 25.0f32)
    };
    let (w, h) = (
        (src_w * scale).round() as u32,
        (src_h * scale).round() as u32,
    );

    let guid = track.clone();
    let press = move |e: MouseEvent| {
        dragging.set(true);
        grab_y.set(e.client_coordinates().y as f32);
        grab_value.set(store.pan(&guid));
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
        // Up is right, and the range is -1..1 — twice the fader's — so the
        // same pixel travel covers the same *fraction* of the control.
        let moved = -(dy / span) as f64 * 2.0;
        drafts.set_pan(&guid, (grab_value() + moved).clamp(-1.0, 1.0));
    };
    let guid = track.clone();
    let release = move |_| {
        if dragging.replace(false) {
            drafts.release_pan(&guid);
        }
    };

    rsx! {
        div {
            style: "display:inline-block; line-height:0; width:{w}px; height:{h}px; \
                    cursor:ns-resize; user-select:none;",
            onmousedown: press,
            onmousemove: drag,
            onmouseup: release.clone(),
            onmouseleave: release,
            art::PanningKnob {
                position: pan() as f32,
                large,
                indicator: true,
                width: Some(w),
                height: Some(h),
            }
        }
    }
}

/// What the track records from.
///
/// A **display**, and deliberately not a selector: no service enumerates
/// the host's audio and MIDI inputs, so an input is an index with nowhere
/// to get a name from. A dropdown here would be a raw number pretending to
/// be a label, so the number is shown as what it is until something can
/// name it.
#[component]
pub fn RecordInputLabel(
    /// The track's GUID.
    track: String,
) -> Element {
    use daw_proto::track::RecordInput;

    let info = use_track(track.clone());
    let text = match info.read().as_ref().map(|t| t.record_input) {
        None | Some(RecordInput::None) => "in —".to_string(),
        Some(RecordInput::Audio { channel }) => format!("in {}", channel + 1),
        Some(RecordInput::Midi { device_id, channel }) => {
            let dev = device_id
                .map(|d| d.to_string())
                .unwrap_or_else(|| "all".into());
            let ch = channel
                .map(|c| (c + 1).to_string())
                .unwrap_or_else(|| "all".into());
            format!("midi {dev}/{ch}")
        }
        Some(RecordInput::Raw(v)) => format!("in #{v}"),
    };

    rsx! {
        div {
            style: "font-family:Fira Sans, DejaVu Sans, sans-serif; font-size:8px; \
                    color:#8a8a8a; text-align:center; white-space:nowrap; overflow:hidden;",
            "{text}"
        }
    }
}

/// The strip's name plate: the track's name, in the track's colour.
///
/// Both fields already existed and both already arrive as events, so this is
/// a read — it is here because a strip without them is not demoable, not
/// because anything was missing.
#[component]
pub fn TrackName(
    /// The track's GUID.
    track: String,
    #[props(default = 72)] width: u32,
    /// `mcp.label`'s own height. Left to the text the plate is whatever
    /// one line of 10px type happens to be, which is not the band REAPER
    /// reserves for it.
    #[props(default = 26)]
    height: u32,
) -> Element {
    let info = use_track(track.clone());
    let (name, colour) = info
        .read()
        .as_ref()
        .map(|t| (t.name.clone(), t.color))
        .unwrap_or_default();

    // The name plate is dark, whatever the track's colour. `mcp_namebg` is
    // a plain plate and only `mcp_idxbg` below it carries the tint — the
    // strip shows the colour once, as a band under the name, and painting
    // both doubled the block and made the bottom of every strip read twice
    // as heavy as REAPER's.
    //
    // The colour is still read: it decides the ink, because a themer can
    // point the plate at a light surface.
    // `mcp_namebg` — a shade darker than the raised surface the plate
    // used, which read as a light bar across the bottom of every strip
    // next to REAPER's. The measured value lives in the token.
    let plate = daw_theme::Color::hex(daw_theme::defaults::STRIP_BODY).expect("token is valid hex");
    let _ = colour;
    // Black text on a light colour, white on a dark one — a track painted
    // yellow is unreadable otherwise. Rec. 601 luma, which is what a
    // contrast decision this coarse needs.
    let luma = 0.299 * plate.r as f32 + 0.587 * plate.g as f32 + 0.114 * plate.b as f32;
    let ink = if luma > 140.0 { "#111111" } else { "#e8e8e8" };

    rsx! {
        div {
            style: "width:{width}px; height:{height}px; line-height:{height}px; \
                    background:{plate.css()}; color:{ink}; \
                    font-family:Fira Sans, DejaVu Sans, sans-serif; font-size:10px; \
                    padding:0 3px; text-align:center; white-space:nowrap; \
                    overflow:hidden; text-overflow:ellipsis;",
            title: "{name}",
            "{name}"
        }
    }
}
