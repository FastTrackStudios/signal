//! The main window — REAPER's shape, composed from the native panels.
//!
//! Transport bar across the top; the track panel down the left with the
//! arrangement beside it; the mixer docked underneath. Every piece already
//! exists — [`NativeTransportBar`], [`TrackRow`], [`ArrangePreview`],
//! [`ChannelStrip`] — so this file is composition and the alignment
//! contract, not new art.
//!
//! # The alignment contract
//!
//! The TCP and the arrange view are two renderings of one row list, so
//! they share one pitch: [`geometry::tcp::ROW_H`] plus a 1px divider. The
//! TCP column carries a spacer the height of the arrange ruler, because in
//! REAPER the first track begins below the ruler line, not beside it.

use std::collections::HashMap;

use crate::components::arrangement_view::{
    ArrangePreview, ArrangeRowKind, EnvelopeLaneView, EnvelopePreview, ItemPreview, plan_rows,
};
use crate::components::media_browser::{MediaBrowserPanel, MediaEntry};
use crate::components::mixer::ChannelStripPreview;
use crate::components::tcp::EnvcpRow;
use crate::components::tcp::TrackRow;
use crate::controls::FxSlotStack;
use crate::panels::native::NativeTransportBar;
use crate::prelude::*;
use daw_proto::Fx;
use daw_proto::{Item, Track};
use daw_theme_art::geometry::tcp::{ROW_H, ROW_W};

/// The arrange ruler's height — stated here as well as in the arrange
/// view because the TCP's spacer must be exactly it.
const RULER_H: f32 = 26.0;
/// The transport bar's band. The bar's art is 26 tall; the band gives it
/// REAPER's breathing room.
const TRANSPORT_H: f32 = 40.0;
/// The docked mixer. Tall enough for the strip to keep its fader
/// (`Collapse` swaps to a knob below ~130 of stretch), short enough to
/// leave the arrangement most of a 768-row window.
const MIXER_H: f32 = 230.0;
/// The FX insert band above the strips, when any track has a chain.
/// Sized for three slot rows plus one expanded embedded GUI.
const FX_BAND_H: f32 = 144.0;
/// The media browser sidebar, when media is passed.
const BROWSER_W: f32 = 260.0;

/// The whole window, for a caller with the data in hand.
///
/// Pure like every `*Preview`: tracks and items in, a window out. The
/// screenshot test paints it; a live wrapper wires the same shape to the
/// DAW.
#[component]
pub fn MainWindowPreview(
    tracks: Vec<Track>,
    #[props(default)] items: Vec<Item>,
    /// Item content previews, by item guid — see `ArrangePreview`.
    #[props(default)]
    previews: HashMap<String, ItemPreview>,
    /// Visible envelopes by track guid — see `ArrangePreview`.
    #[props(default)]
    envelopes: HashMap<String, Vec<EnvelopePreview>>,
    /// Envelopes in their own lanes, by track guid — the TCP column grows
    /// an envcp row per lane, on the shared plan.
    #[props(default)]
    env_lanes: HashMap<String, Vec<EnvelopeLaneView>>,
    /// FX chains by track guid. Any non-empty chain grows the FX band
    /// above the mixer strips.
    #[props(default)]
    fx: HashMap<String, Vec<Fx>>,
    /// The initially expanded FX slot: `(track guid, fx guid)`.
    #[props(default)]
    fx_expanded: Option<(String, String)>,
    /// Media browser library entries. Empty hides the sidebar entirely.
    #[props(default)]
    media: Vec<MediaEntry>,
    /// The browser's selected entry, by name.
    #[props(default)]
    media_selected: Option<String>,
    #[props(default = 1024.0)] width: f32,
    #[props(default = 768.0)] height: f32,
    #[props(default = 120.0)] bpm: f64,
) -> Element {
    let t = daw_theme::Theme::default();
    let ground = t.chrome.surface.css();
    let bar_bg = t.chrome.surface_sunken.shade(-0.05).css();
    let rule = t.chrome.surface_sunken.shade(-0.25).css();

    let playing = use_signal(|| false);
    // Static — this is a pure preview/screenshot composition, not a live
    // transport. `ArrangePreview` takes a signal (not a plain `f32`) so
    // the live caller (`ArrangementView`) can update position without
    // re-rendering its whole tree on every tick; a preview just never
    // writes to it.
    let cursor_position = use_signal(|| 0.0f32);

    let fx_band = if fx.values().any(|c| !c.is_empty()) {
        FX_BAND_H
    } else {
        0.0
    };
    let middle_h = height - TRANSPORT_H - MIXER_H - fx_band;
    // The browser takes its column off the project area, full height
    // under the transport — a sidebar, not a fourth dock row.
    let browser_w = if media.is_empty() { 0.0 } else { BROWSER_W };
    let project_w = width - browser_w;
    let arrange_w = project_w - ROW_W;

    rsx! {
        div {
            style: "position:relative; width:{width}px; height:{height}px; \
                    background:{ground}; overflow:hidden; \
                    display:flex; flex-direction:column;",

            // ── Transport ──
            div {
                style: "height:{TRANSPORT_H}px; flex:0 0 auto; display:flex; \
                        align-items:center; padding-left:8px; \
                        background:{bar_bg}; border-bottom:1px solid {rule};",
                NativeTransportBar { playing, bpm }
            }

            // ── Project | media browser ──
            div {
                style: "flex:1 1 0; min-height:0; display:flex; overflow:hidden;",

                div {
                    style: "width:{project_w}px; flex:0 0 auto; display:flex; \
                            flex-direction:column; overflow:hidden;",

                    // ── TCP | arrangement ──
                    div {
                        style: "height:{middle_h}px; flex:0 0 auto; display:flex; \
                                overflow:hidden;",
                        div {
                            style: "width:{ROW_W}px; flex:0 0 auto; overflow:hidden;",
                            // The ruler's height in empty panel, so row one
                            // starts where lane one does.
                            div { style: "height:{RULER_H + 1.0}px;" }
                            // Both sides walk plan_rows, so an envcp row
                            // is exactly as tall as the lane it controls.
                            for (kind, _, h) in
                                plan_rows(&tracks, &env_lanes, daw_theme_art::geometry::tcp::ROW_H)
                            {
                                match kind {
                                    ArrangeRowKind::Track(i) => rsx! {
                                        TrackRow {
                                            key: "{tracks[i].guid}",
                                            track: tracks[i].clone(),
                                            index: i as u32,
                                            depth: crate::components::arrangement_view::folder_depths(&tracks)[i],
                                        }
                                    },
                                    ArrangeRowKind::EnvelopeLane { track, lane } => {
                                        let view = &env_lanes[&tracks[track].guid][lane];
                                        let fx_param = view.envelope.name.contains('/');
                                        rsx! {
                                            EnvcpRow {
                                                key: "e{track}-{lane}",
                                                name: view.envelope.name.clone(),
                                                height: h,
                                                fx_param,
                                                armed: true,
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        ArrangePreview {
                            tracks: tracks.clone(),
                            items,
                            previews,
                            envelopes,
                            env_lanes,
                            width: arrange_w,
                            height: middle_h,
                            bpm,
                            play_position: cursor_position,
                            edit_position: cursor_position,
                        }
                    }

                    // ── The FX insert band, above the strips ──
                    //
                    // REAPER's MCP puts the chain above the strip; ours
                    // is a band across the dock so an expanded embedded
                    // GUI has the full slot column to grow into.
                    if fx_band > 0.0 {
                        div {
                            style: "height:{FX_BAND_H}px; flex:0 0 auto; display:flex; \
                                    align-items:flex-end; border-top:1px solid {rule}; \
                                    overflow:hidden; background:{bar_bg};",
                            for track in tracks.iter() {
                                {
                                    let chain = fx.get(&track.guid).cloned().unwrap_or_default();
                                    let open = fx_expanded
                                        .as_ref()
                                        .filter(|(t, _)| t == &track.guid)
                                        .map(|(_, f)| f.clone());
                                    rsx! {
                                        div {
                                            key: "{track.guid}",
                                            style: "width:{daw_theme_art::geometry::mcp::STRIP_W}px; \
                                                    flex:0 0 auto; overflow:hidden;",
                                            FxSlotStack {
                                                fx: chain,
                                                width: daw_theme_art::geometry::mcp::STRIP_W,
                                                expanded: open,
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // ── The docked mixer ──
                    div {
                        style: "height:{MIXER_H}px; flex:0 0 auto; display:flex; \
                                border-top:1px solid {rule}; overflow:hidden; \
                                background:{bar_bg};",
                        for (i, track) in tracks.iter().enumerate() {
                            ChannelStripPreview {
                                key: "{track.guid}",
                                track: track.clone(),
                                index: i as u32,
                                height: MIXER_H,
                            }
                        }
                    }
                }

                if !media.is_empty() {
                    MediaBrowserPanel {
                        library: media,
                        selected: media_selected,
                        width: BROWSER_W,
                        height: height - TRANSPORT_H,
                    }
                }
            }
        }
    }
}
