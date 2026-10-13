//! The mute button, live.
//!
//! The tracer bullet for the whole vector-theme effort: one control that a
//! mixing engineer can hover, click and see toggle in the browser, in the
//! desktop app and inside a REAPER panel — where the *same* component is
//! what the exporter rasterises into REAPER's three-cell mute sprite.

use daw_theme_art::dress::{self, Panel};
use daw_theme_art::vector_controls as art;

use crate::controls::reach::write_track;
use crate::controls::track_store::use_track;
use crate::prelude::*;

/// A track's mute button, wired to the DAW.
///
/// Owns the pointer state, reads the track from [`crate::controls::TrackStore`],
/// and toggles through `daw_control` on press. It does not own the mute
/// *state*: the click asks the backend, and the button lights when the
/// backend's event comes back — which is also what makes it light when the
/// track is muted from REAPER's own menu.
#[component]
pub fn MuteButton(
    /// The track's GUID.
    track: String,
    #[props(default)] panel: Panel,
    /// Pixels per art pixel. 1.0 draws the art at its measured size.
    #[props(default = 1.0)]
    scale: f32,
) -> Element {
    // Hover is a Signal here rather than a `:hover` rule in a stylesheet,
    // and that is forced, not chosen. Every non-browser target hands the
    // `<svg>` subtree to a parser where `:hover` is inert, so nothing
    // *inside* the art can know it is hovered. The state has to be owned out
    // here and handed in as a prop — the same prop the exporter sets three
    // ways to draw a sprite strip.
    let mut at = use_signal(art::Interaction::default);
    let muted = use_track(track.clone());
    let muted = muted.read().as_ref().is_some_and(|t| t.muted);

    let named = dress::mute_art(panel, muted);
    let (w, h) = named.source;
    let (w, h) = ((w * scale).round() as u32, (h * scale).round() as u32);

    let guid = track.clone();
    let press = move |_| {
        at.set(art::Interaction::Pressed);
        write_track(
            guid.clone(),
            "mute",
            |t| async move { t.toggle_mute().await },
        );
    };

    rsx! {
        div {
            // Explicit, inline, and in pixels — layout must not wait for a
            // stylesheet that Blitz may never load. `line-height: 0` keeps
            // the inline `<svg>` from sitting on a text baseline and
            // growing the box by a few pixels, which in a mixer strip is
            // the difference between the buttons lining up and not.
            style: "display:inline-block; line-height:0; width:{w}px; height:{h}px; cursor:pointer;",
            onmouseenter: move |_| at.set(art::Interaction::Hover),
            // Leaving while held must not strand the button in `Pressed`:
            // the pointer is gone and no `mouseup` is coming here.
            onmouseleave: move |_| at.set(art::Interaction::Normal),
            onmousedown: press,
            onmouseup: move |_| at.set(art::Interaction::Hover),
            art::MuteButton {
                width: Some(w),
                height: Some(h),
                at: at(),
                ..dress::mute(named, muted)
            }
        }
    }
}
