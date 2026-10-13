//! The arrangement view — ruler, lanes, items — drawn native.
//!
//! The third panel of the main window, beside `components::tcp` and above
//! `components::mixer`, built the same way: explicit inline layout on the
//! theme's tokens, no stylesheet assumed, no WALTER, no bitmaps. The lanes
//! share the TCP's row pitch — [`geometry::tcp::ROW_H`] plus its divider —
//! so a row in one panel is the same row in the other, which is the whole
//! contract between them.
//!
//! # What Blitz allows here
//!
//! Checked against blitz.is/status/css before this was built: flexbox and
//! absolute positioning are solid, `overflow:scroll` works but `auto` does
//! not, and there is no `position:sticky` — so the ruler is a sibling of
//! the lane area rather than a sticky header, and scrolling is deferred
//! until the panel needs it (the preview draws a window, not a viewport).

use std::collections::HashMap;

use crate::components::tcp::{EnvcpRow, TrackRow};
use crate::components::toolbars::{
    KeybindProfile, ModeDropdown, ModeOption, ProfilePicker, RightToolbar, ToolbarAction,
    TopToolbar,
};
use crate::controls::{use_daw_tracks, use_track_store};
use crate::prelude::*;
use daw_proto::{Item, Track};
use daw_theme_art::geometry::tcp::ROW_W;
use input::InputCommand;
use input_dioxus::use_input_processor;

/// What an item shows of its content — REAPER's waveform or note preview.
///
/// Carried per item, keyed by guid, and *pre-resolved to render units*:
/// amplitudes 0..1 for audio, seconds-from-item-start for notes. The
/// fetch does the PPQ and tempo arithmetic once so the render is a pure
/// projection, and a fixture can state a preview without a tempo map.
#[derive(Clone, PartialEq, Debug)]
pub enum ItemPreview {
    /// Peak amplitudes, 0..1, evenly spaced across the take — one per
    /// block, drawn stretched to the item's box and mirrored about the
    /// centre line. Under REAPER these come off the `.reapeaks` cache
    /// via `PCM_source::GetPeaks` (see `TakeHandle::peaks`).
    Waveform(Vec<f32>),
    /// MIDI notes, in seconds from the item start.
    Notes(Vec<NotePreview>),
}

/// One note in a MIDI preview.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct NotePreview {
    pub pitch: u8,
    /// Seconds from the item's start.
    pub start: f32,
    /// Seconds.
    pub length: f32,
}

/// Fold a take's peak frame into per-block amplitudes.
///
/// The REAPER backend returns one peak per channel per block (interleaved
/// `[ch0, ch1, ch0, ch1, …]`); the preview takes the loudest channel and
/// mirrors it, which is how a single-lane waveform is usually drawn.
pub fn waveform_from_peaks(data: &daw_proto::TakePeakData) -> Vec<f32> {
    let ch = data.num_channels.max(1) as usize;
    data.peaks
        .chunks(ch)
        .map(|block| {
            block
                .iter()
                .fold(0.0f32, |a, v| a.max(v.abs() as f32))
                .min(1.0)
        })
        .collect()
}

/// One visible envelope on a track's lane, pre-resolved to render units.
///
/// The same currency idea as [`ItemPreview`]: `(seconds, 0..1)` points,
/// so the render is a projection and a fixture needs no automation API.
/// Values arrive normalised from `daw_proto::EnvelopePoint` already.
#[derive(Clone, PartialEq, Debug)]
pub struct EnvelopePreview {
    /// The envelope's display name — also decides its colour, the way
    /// REAPER inks volume green and pan orange.
    pub name: String,
    /// `(time in seconds, value 0..1)`, in time order.
    pub points: Vec<(f32, f32)>,
}

impl EnvelopePreview {
    /// REAPER's convention, in the theme's tokens: volume is the meter
    /// green, pan the meter amber, anything else the accent.
    fn colour(&self, t: &daw_theme::Theme) -> daw_theme::Color {
        let name = self.name.to_ascii_lowercase();
        if name.contains("volume") {
            t.signal.meter_safe
        } else if name.contains("pan") || name.contains("width") {
            t.signal.meter_warn
        } else {
            t.chrome.accent
        }
    }
}

/// An envelope shown in its **own lane** under the track, rather than
/// overlaid — REAPER's per-envelope lanes, vertically resizable, where FX
/// parameter automation lives.
#[derive(Clone, PartialEq, Debug)]
pub struct EnvelopeLaneView {
    pub envelope: EnvelopePreview,
    /// The lane's height. REAPER's floor is `envcp_min_height 27` (from
    /// `rtconfig.txt`); resizing changes this number and everything else
    /// follows.
    pub height: f32,
    /// Automation items sitting on this lane.
    pub automation_items: Vec<AutomationItemView>,
}

/// One automation item: a windowed piece of envelope with its own header,
/// movable and poolable like a media item.
#[derive(Clone, PartialEq, Debug)]
pub struct AutomationItemView {
    pub name: String,
    /// Seconds on the timeline.
    pub start: f32,
    /// Seconds.
    pub length: f32,
    /// A pooled instance edits its siblings too — marked in the header.
    pub pooled: bool,
    /// `(seconds from the item's start, value 0..1)`.
    pub points: Vec<(f32, f32)>,
}

/// A row in the arrange's vertical layout — a track's lane or one of its
/// envelope lanes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ArrangeRowKind {
    Track(usize),
    /// `lane` indexes into the track's `EnvelopeLaneView` list.
    EnvelopeLane {
        track: usize,
        lane: usize,
    },
}

/// The arrange's vertical plan: every row with its top and height, tracks
/// interleaved with their envelope lanes.
///
/// **This is the alignment contract between the arrange and the TCP
/// column**, extended from "same pitch" to "same plan": both sides walk
/// this list, so an envelope lane's envcp row is exactly as tall as the
/// lane it controls, the way REAPER ties `envcp` height to the lane.
pub fn plan_rows(
    tracks: &[Track],
    lanes: &HashMap<String, Vec<EnvelopeLaneView>>,
    row_h: f32,
) -> Vec<(ArrangeRowKind, f32, f32)> {
    let mut out = Vec::new();
    let mut top = 0.0f32;
    for (i, track) in tracks.iter().enumerate() {
        out.push((ArrangeRowKind::Track(i), top, row_h));
        top += row_h + 1.0;
        if let Some(track_lanes) = lanes.get(&track.guid) {
            for (l, lane) in track_lanes.iter().enumerate() {
                let h = lane.height.max(ENVELOPE_LANE_MIN_H);
                out.push((ArrangeRowKind::EnvelopeLane { track: i, lane: l }, top, h));
                top += h + 1.0;
            }
        }
    }
    out
}

/// Each track's REAPER-style folder indent — one entry per `tracks[i]`.
///
/// `Track::folder_depth` is REAPER's raw delta encoding: positive N on a
/// track means "this track opens N folder level(s), starting with the
/// NEXT track"; negative N means "this closes N level(s) after this
/// track". A track's OWN indent is the running depth *before* its delta
/// applies — a folder-start track sits at its parent's depth, and its
/// children (the following tracks, until the levels it opened are
/// closed) sit one level deeper. Kept as a side table rather than folded
/// into `plan_rows`'s tuple shape, to avoid rippling into its two
/// existing consumers (this file's TCP column, `main_window.rs`'s).
pub fn folder_depths(tracks: &[Track]) -> Vec<u32> {
    let mut depths = Vec::with_capacity(tracks.len());
    let mut depth: i32 = 0;
    for track in tracks {
        depths.push(depth.max(0) as u32);
        depth += track.folder_depth;
    }
    depths
}

/// `envcp_min_height` from the theme — the floor a lane cannot resize
/// below.
pub const ENVELOPE_LANE_MIN_H: f32 = 27.0;

/// Where the checked-in keybind profiles live — `reaper-input`'s own
/// config directory, a sibling crate in this same repo. Computed off
/// this crate's manifest dir rather than a relative runtime path, since
/// the working directory at launch is whatever the host app happened to
/// start in, not this crate's source location.
fn profiles_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reaper/reaper-input/config/config")
}

/// Load one profile's keymap by slug — falls back to an empty (no
/// bindings) config if the profile is missing/unparseable rather than
/// erroring, matching `KeybindProfile::list`'s own quiet-skip behavior.
fn load_keymap(slug: &str) -> input::KeymapConfig {
    input_keybinds::load_profile_keymap(&profiles_root().join(slug)).unwrap_or_default()
}

/// The ruler's band, above the lanes.
const RULER_H: f32 = 26.0;

/// The markers/regions band, directly below the ruler — REAPER's own
/// marker/region lane. Only rendered in `ArrangementView`'s live,
/// unified-scroll layout (see its module doc); `ArrangePreview`'s
/// screenshot path doesn't have live marker/region data to show.
const MARKER_BAND_H: f32 = 20.0;

/// One second of timeline, in pixels, at the default zoom.
///
/// Chosen so a 4/4 bar at 120 BPM is 80px — items read at a glance and an
/// eight-bar loop fits a small window. Zoom is a prop; this is its seed.
const PX_PER_SECOND: f32 = 40.0;

/// The arrange area for a caller that has the data in hand.
///
/// Pure, like `ChannelStripPreview` and `TrackRow`: tracks and items in,
/// markup out, no backend, no context — so a test, a screenshot or the
/// main-window composition can draw it without a DAW attached.
/// [`ArrangementView`] is the live wrapper.
#[component]
pub fn ArrangePreview(
    tracks: Vec<Track>,
    #[props(default)] items: Vec<Item>,
    /// Item content previews, by item guid. An item with no entry draws
    /// its plain block — previews arrive as they are fetched.
    #[props(default)]
    previews: HashMap<String, ItemPreview>,
    /// Visible envelopes, by track guid, drawn over the lane the way
    /// REAPER overlays them.
    #[props(default)]
    envelopes: HashMap<String, Vec<EnvelopePreview>>,
    /// Envelopes in their **own lanes** under the track, by track guid —
    /// where FX parameter automation and automation items live. Heights
    /// come with the data; [`plan_rows`] turns them into the vertical
    /// plan both this panel and the TCP column walk.
    #[props(default)]
    env_lanes: HashMap<String, Vec<EnvelopeLaneView>>,
    width: f32,
    height: f32,
    /// Zoom, as pixels per second of timeline.
    #[props(default = PX_PER_SECOND)]
    pixels_per_second: f32,
    /// The tempo the ruler's bars are spaced by. One number for now — a
    /// tempo *map* changes the arithmetic from a stride to a walk, and
    /// nothing here draws tempo changes yet.
    #[props(default = 120.0)]
    bpm: f64,
    /// The lane pitch. Defaults to the TCP's row so the two panels line
    /// up; the caller that draws taller rows passes what it drew.
    #[props(default = daw_theme_art::geometry::tcp::ROW_H)]
    row_h: f32,
    /// A lane was dragged to a new height: `(track guid, lane index,
    /// height)`. `None` makes the lanes fixed, which is what a
    /// screenshot wants.
    #[props(default)]
    on_lane_resize: Option<EventHandler<(String, usize, f32)>>,
    /// Playback cursor, in seconds — a `Signal`, not a plain `f32`, and
    /// read ONLY inside the small `PositionCursor` leaf this renders,
    /// never here. A live position tick lands at ~30Hz; if this
    /// component's own body read the value (to pass a plain `f32` down),
    /// Dioxus would re-run this ENTIRE render — every track row, every
    /// bar/beat line, every item — 30 times a second, which is exactly
    /// what made the arrange view unusably slow. Pushing the read down to
    /// a component that renders nothing but a 1px div means only that
    /// div's own tiny render re-runs on each tick.
    play_position: ReadSignal<f32>,
    /// Edit cursor, in seconds — independent of the playhead while
    /// transport runs. Same reasoning as `play_position`.
    edit_position: ReadSignal<f32>,
    /// Fired with `(scroll_left, scroll_top)` on every scroll of the
    /// lanes area — `ArrangementView` mirrors the vertical component into
    /// the TCP column, which lives outside this component and has no
    /// other way to stay lined up with the lanes as they scroll.
    #[props(default)]
    on_scroll: Option<EventHandler<(f32, f32)>>,
) -> Element {
    let t = daw_theme::Theme::default();
    // The arrange ground is the sunken surface — the one place the theme
    // reaches *down* the ladder, which is what makes the panels around it
    // read as raised.
    let ground = t.chrome.surface_sunken.css();
    let ruler_bg = t.chrome.surface_sunken.shade(-0.12).css();
    let ruler_ink = t.chrome.text_dim.css();
    let rule = t.chrome.surface_sunken.shade(-0.25).css();
    // Bar lines are cut into the ground, beat lines barely so.
    let bar_line = t.chrome.surface_sunken.shade(-0.18).css();
    let beat_line = t.chrome.surface_sunken.shade(-0.07).css();
    let lane_rule = t.chrome.surface_sunken.shade(-0.15).css();
    // The richer arrange-specific theme (REAPER's `col_cursor`/
    // `playcursor_color` vocabulary) carries real cursor colors that
    // nothing in this file used before — everything else here stays on
    // the simpler `daw_theme::Theme` it already uses.
    let arrange_theme = crate::theming::Theme::default();
    let edit_cursor_colour = arrange_theme.arrange.edit_cursor.css();
    let play_cursor_colour = arrange_theme.arrange.play_cursor.css();

    // The lanes area is a REAL scrollable region (native `overflow:scroll`
    // — always-visible scrollbars, per the module doc: Blitz supports
    // `scroll`, not `auto`, and this is what the user actually asked for
    // — a fixed-size "camera" with visible bars showing there's more to
    // scroll to, not the box itself growing to fit the content). The
    // ruler has no scrollbar of its own; it mirrors the lanes' `onscroll`
    // via `scroll_x` the same way `on_scroll` lets `ArrangementView`
    // mirror it into the TCP column, which lives outside this component
    // entirely and has no other way to stay lined up with the lanes.
    let mut scroll_x = use_signal(|| 0.0f32);

    // 4/4 until the tempo map is wired; the ruler numbers measures.
    let bar_secs = 240.0 / bpm.max(1.0);
    let bar_px = bar_secs as f32 * pixels_per_second;
    // Beat lines only when they have room to be lines rather than moiré.
    let beat_px = bar_px / 4.0;
    let draw_beats = beat_px >= 12.0;

    let lanes_h = height - RULER_H;

    // The vertical plan: tracks interleaved with their envelope lanes.
    let plan = plan_rows(&tracks, &env_lanes, row_h);
    let track_top = |i: usize| -> f32 {
        plan.iter()
            .find(|(k, _, _)| *k == ArrangeRowKind::Track(i))
            .map(|(_, top, _)| *top)
            .unwrap_or(i as f32 * (row_h + 1.0))
    };

    // The full scrollable canvas — NOT the viewport. Wide enough to cover
    // every item plus a screen's worth of run-off room to scroll into
    // (REAPER always leaves some blank timeline past the last item), and
    // never narrower than the viewport itself so an empty/short project
    // doesn't render a canvas smaller than its own window.
    let content_secs = items
        .iter()
        .map(|it| it.position.as_seconds() as f32 + it.length.as_seconds() as f32)
        .fold(0.0f32, f32::max)
        + width / pixels_per_second.max(1.0);
    let content_w = (content_secs * pixels_per_second).max(width);
    let content_h = plan
        .iter()
        .map(|(_, top, h)| top + h)
        .fold(0.0f32, f32::max)
        .max(lanes_h);
    let bars = (content_w / bar_px).ceil() as usize + 1;

    rsx! {
        div {
            style: "position:relative; width:{width}px; height:{height}px; \
                    background:{ground}; overflow:hidden; \
                    display:flex; flex-direction:column;",

            // ── The ruler ── no scrollbar of its own; mirrors the lanes'
            // real horizontal scroll via `scroll_x`.
            div {
                style: "position:relative; height:{RULER_H}px; flex:0 0 auto; \
                        background:{ruler_bg}; border-bottom:1px solid {rule}; \
                        overflow:hidden;",
                div {
                    style: "position:relative; transform:translateX(-{scroll_x()}px);",
                    for bar in 0..bars {
                        div {
                            key: "r{bar}",
                            style: "position:absolute; left:{bar as f32 * bar_px}px; top:0; \
                                    height:{RULER_H}px; border-left:1px solid {bar_line}; \
                                    padding-left:4px; font-size:9px; \
                                    line-height:{RULER_H}px; color:{ruler_ink}; \
                                    font-family:Fira Sans, DejaVu Sans, sans-serif;",
                            "{bar + 1}"
                        }
                    }
                }
            }

            // ── The lanes ── the real scrollable "camera": fixed at
            // `{width}x{lanes_h}`, `overflow:scroll` so both bars are
            // always visible (Blitz supports `scroll`, not `auto` — see
            // the module doc — which suits this anyway: the user wants
            // the scrollbars to show there's more to see, not to hide
            // until needed). The inner canvas below is what actually
            // grows with content; this viewport never does.
            div {
                style: "position:relative; flex:1 1 0; min-height:0; \
                        overflow:scroll; cursor:default;",
                onscroll: move |evt: ScrollEvent| {
                    let x = evt.scroll_left() as f32;
                    let y = evt.scroll_top() as f32;
                    scroll_x.set(x);
                    if let Some(cb) = on_scroll {
                        cb.call((x, y));
                    }
                },
                ArrangeCanvas {
                    tracks: tracks.clone(),
                    items: items.clone(),
                    previews: previews.clone(),
                    envelopes: envelopes.clone(),
                    env_lanes: env_lanes.clone(),
                    width: content_w,
                    height: content_h,
                    pixels_per_second,
                    row_h,
                    bar_px,
                    beat_px,
                    draw_beats,
                    bars,
                    on_lane_resize,
                    play_position,
                    edit_position,
                }
            }
        }
    }
}

/// A single 1px cursor line, positioned from a `ReadSignal` it reads
/// itself — see `ArrangePreview`'s `play_position` doc for why. Reading
/// the signal HERE, in a component whose entire render is this one div,
/// means a ~30Hz position tick re-renders only this, not the arrange
/// view's whole track/item/bar tree.
#[component]
fn PositionCursor(
    position: ReadSignal<f32>,
    pixels_per_second: f32,
    height: f32,
    colour: String,
) -> Element {
    rsx! {
        div {
            style: "position:absolute; top:0; left:{position() * pixels_per_second}px; \
                    width:1px; height:{height}px; background:{colour}; \
                    pointer-events:none;",
        }
    }
}

/// The arrange lanes' actual content — grid, per-track tint, items,
/// envelopes, envelope lanes, cursors — sized to the full scrollable
/// canvas (`width`/`height` here are the CONTENT size, not a viewport).
/// Extracted so both `ArrangePreview`'s own scroll box (screenshots) and
/// `ArrangementView`'s unified TCP+lanes scroll surface (its module doc
/// explains why there's only one scroll there, not two kept in sync) can
/// render identical content without duplicating this.
#[component]
fn ArrangeCanvas(
    tracks: Vec<Track>,
    #[props(default)] items: Vec<Item>,
    #[props(default)] previews: HashMap<String, ItemPreview>,
    #[props(default)] envelopes: HashMap<String, Vec<EnvelopePreview>>,
    #[props(default)] env_lanes: HashMap<String, Vec<EnvelopeLaneView>>,
    /// The content canvas size — NOT a viewport; the caller already
    /// worked out how much timeline/track space there is to draw.
    width: f32,
    height: f32,
    #[props(default = PX_PER_SECOND)] pixels_per_second: f32,
    #[props(default = daw_theme_art::geometry::tcp::ROW_H)] row_h: f32,
    bar_px: f32,
    beat_px: f32,
    draw_beats: bool,
    bars: usize,
    #[props(default)] on_lane_resize: Option<EventHandler<(String, usize, f32)>>,
    play_position: ReadSignal<f32>,
    edit_position: ReadSignal<f32>,
    /// Fired with an item's guid on plain click (select), or with `None`
    /// on a click that hit empty lane space (deselect all) — REAPER's
    /// `MM_CTX_ITEMLOWER_CLK` / `MM_CTX_TRACK_CLK` defaults.
    #[props(default)]
    on_item_click: Option<EventHandler<Option<String>>>,
) -> Element {
    let t = daw_theme::Theme::default();
    let bar_line = t.chrome.surface_sunken.shade(-0.18).css();
    let beat_line = t.chrome.surface_sunken.shade(-0.07).css();
    let lane_rule = t.chrome.surface_sunken.shade(-0.15).css();
    let arrange_theme = crate::theming::Theme::default();
    let edit_cursor_colour = arrange_theme.arrange.edit_cursor.css();
    let play_cursor_colour = arrange_theme.arrange.play_cursor.css();

    let content_w = width;
    let content_h = height;
    let plan = plan_rows(&tracks, &env_lanes, row_h);
    let track_top = |i: usize| -> f32 {
        plan.iter()
            .find(|(k, _, _)| *k == ArrangeRowKind::Track(i))
            .map(|(_, top, _)| *top)
            .unwrap_or(i as f32 * (row_h + 1.0))
    };

    rsx! {
        div {
            style: "position:relative; width:{content_w}px; height:{content_h}px;",

            // The grid, behind everything: a line per bar, and per
            // beat when the zoom leaves room for them.
            for bar in 1..bars {
                div {
                    key: "b{bar}",
                    style: "position:absolute; left:{bar as f32 * bar_px}px; top:0; \
                            width:1px; height:{content_h}px; background:{bar_line};",
                }
            }
            if draw_beats {
                for beat in 1..bars * 4 {
                    if beat % 4 != 0 {
                        div {
                            key: "t{beat}",
                            style: "position:absolute; left:{beat as f32 * beat_px}px; \
                                    top:0; width:1px; height:{content_h}px; \
                                    background:{beat_line};",
                        }
                    }
                }
            }

            // A lane per track, on the TCP's pitch: row plus one
            // divider. The lane carries the track's colour as a tint
            // faint enough to organise without shouting.
            for (i, track) in tracks.iter().enumerate() {
                {
                    let top = track_top(i);
                    let tint = track
                        .color
                        .map(|c| format!(
                            "rgba({}, {}, {}, 0.05)",
                            (c >> 16) & 0xff, (c >> 8) & 0xff, c & 0xff
                        ))
                        .unwrap_or_else(|| "transparent".to_string());
                    rsx! {
                        div {
                            key: "{track.guid}",
                            style: "position:absolute; left:0; top:{top}px; \
                                    width:{content_w}px; height:{row_h}px; \
                                    background:{tint}; \
                                    border-bottom:1px solid {lane_rule};",
                            // Empty lane space, plain click — REAPER's
                            // `MM_CTX_TRACK_CLK` default ("Deselect all
                            // items"). Items sit above this div and stop
                            // their own clicks from reaching it.
                            onclick: move |_| {
                                if let Some(cb) = on_item_click {
                                    cb.call(None);
                                }
                            },
                        }
                    }
                }
            }

            // The items, over the lanes and the grid.
            for item in items.iter() {
                {
                    let lane = tracks.iter().position(|tr| tr.guid == item.track_guid);
                    match lane {
                        Some(i) => rsx! {
                            ArrangeItem {
                                key: "{item.guid}",
                                item: item.clone(),
                                track_colour: tracks[i].color,
                                top: track_top(i),
                                height: row_h,
                                pixels_per_second,
                                preview: previews.get(&item.guid).cloned(),
                                on_click: move |guid: String| {
                                    if let Some(cb) = on_item_click {
                                        cb.call(Some(guid));
                                    }
                                },
                            }
                        },
                        // An item whose track is not in the list draws
                        // nothing — better than inventing a lane.
                        None => rsx! {},
                    }
                }
            }

            // Envelopes over their lanes, above the items — REAPER's
            // overlay order.
            for (guid, lanes) in envelopes.iter() {
                {
                    let lane = tracks.iter().position(|tr| &tr.guid == guid);
                    match lane {
                        Some(i) => rsx! {
                            for (e, env) in lanes.iter().enumerate() {
                                EnvelopeLane {
                                    key: "{guid}/{e}",
                                    envelope: env.clone(),
                                    top: track_top(i),
                                    height: row_h,
                                    width: content_w,
                                    pixels_per_second,
                                }
                            }
                        },
                        None => rsx! {},
                    }
                }
            }

            // The envelope lanes themselves: a darker ground than the
            // track lanes, the envelope across it, automation items
            // as blocks on top.
            for (kind, top, h) in plan.iter() {
                if let ArrangeRowKind::EnvelopeLane { track, lane } = kind {
                    {
                        let view = &env_lanes[&tracks[*track].guid][*lane];
                        rsx! {
                            div {
                                key: "el{track}-{lane}",
                                style: "position:absolute; left:0; top:{top}px; \
                                        width:{content_w}px; height:{h}px; \
                                        background:rgba(0,0,0,0.18); \
                                        border-bottom:1px solid {lane_rule};",
                            }
                            EnvelopeLane {
                                envelope: view.envelope.clone(),
                                top: *top,
                                height: *h,
                                width: content_w,
                                pixels_per_second,
                            }
                            for (a, ai) in view.automation_items.iter().enumerate() {
                                AutomationItemBlock {
                                    key: "ai{track}-{lane}-{a}",
                                    item: ai.clone(),
                                    top: *top,
                                    height: *h,
                                    pixels_per_second,
                                    colour: view.envelope.colour(&daw_theme::Theme::default()),
                                }
                            }
                            if let Some(resize) = on_lane_resize {
                                {
                                    let guid = tracks[*track].guid.clone();
                                    let lane = *lane;
                                    rsx! {
                                        LaneResizeHandle {
                                            height: *h,
                                            width: content_w,
                                            top: *top,
                                            on_resize: move |h: f32| {
                                                resize.call((guid.clone(), lane, h))
                                            },
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Edit cursor, then play cursor on top (REAPER's own
            // z-order — the playhead reads over a coincident edit
            // cursor since it's the one that's usually moving). Each
            // is its own leaf component so the ~30Hz position ticks
            // re-render a single 1px div, not this whole tree — see
            // `PositionCursor`'s doc.
            PositionCursor {
                position: edit_position,
                pixels_per_second,
                height: content_h,
                colour: edit_cursor_colour,
            }
            PositionCursor {
                position: play_position,
                pixels_per_second,
                height: content_h,
                colour: play_cursor_colour,
            }
        }
    }
}

/// One item on a lane.
///
/// REAPER's item is a block of the track's colour with a thin darker
/// outline and its label set small in the top-left; muted items fade and
/// a selected item brightens its border. The fades and per-take lanes
/// come later — this is the block.
#[component]
fn ArrangeItem(
    item: Item,
    track_colour: Option<u32>,
    top: f32,
    height: f32,
    pixels_per_second: f32,
    #[props(default)] preview: Option<ItemPreview>,
    /// Fired with the item's guid on plain click — REAPER's own default
    /// `MM_CTX_ITEMLOWER_CLK` behavior ("select item"). `None` draws a
    /// non-interactive item (`MainWindowPreview`'s screenshot use).
    #[props(default)]
    on_click: Option<EventHandler<String>>,
) -> Element {
    let t = daw_theme::Theme::default();
    let left = item.position.as_seconds() as f32 * pixels_per_second;
    let w = (item.length.as_seconds() as f32 * pixels_per_second).max(2.0);

    // The item's own colour beats the track's, as in REAPER.
    let colour = item.color.or(track_colour);
    let body = colour
        .map(|c| {
            daw_theme_art::dress::panel_tint(daw_theme::Color::rgb(
                (c >> 16) as u8,
                (c >> 8) as u8,
                c as u8,
            ))
        })
        .unwrap_or(t.signal.neutral_track);
    let edge = if item.selected {
        t.chrome.selected.css()
    } else {
        body.shade(-0.45).css()
    };
    let ink = body.shade(0.6).css();
    let alpha = if item.muted { 0.45 } else { 1.0 };
    let label = item.label.clone().unwrap_or_default();
    let box_h = height - 3.0;
    // The content band: the label's row belongs to the label, the rest to
    // the preview, which is how REAPER divides an item.
    let band_top = if label.is_empty() { 2.0 } else { 12.0 };
    let band_h = (box_h - band_top - 2.0).max(0.0);
    // Peaks are the item's own colour pulled dark — content on the block,
    // not a second surface.
    let mark = body.shade(-0.38);

    let guid = item.guid.clone();
    let cursor = if on_click.is_some() {
        "pointer"
    } else {
        "default"
    };
    rsx! {
        div {
            style: "position:absolute; left:{left}px; top:{top + 1.0}px; \
                    width:{w}px; height:{box_h}px; \
                    background:{body.css()}; border:1px solid {edge}; \
                    border-radius:3px; opacity:{alpha}; overflow:hidden; \
                    cursor:{cursor};",
            onclick: move |_| {
                if let Some(cb) = on_click {
                    cb.call(guid.clone());
                }
            },
            match &preview {
                Some(ItemPreview::Waveform(amps)) if !amps.is_empty() && band_h > 3.0 => rsx! {
                    Waveform {
                        amps: amps.clone(),
                        width: w,
                        top: band_top,
                        height: band_h,
                        colour: mark.css(),
                    }
                },
                Some(ItemPreview::Notes(notes)) if !notes.is_empty() && band_h > 3.0 => rsx! {
                    NoteRoll {
                        notes: notes.clone(),
                        length: item.length.as_seconds() as f32,
                        width: w,
                        top: band_top,
                        height: band_h,
                        colour: mark.css(),
                    }
                },
                _ => rsx! {},
            }
            if !label.is_empty() {
                div {
                    style: "position:absolute; left:0; top:0; width:{w}px; \
                            padding:1px 4px; font-size:9px; line-height:11px; \
                            color:{ink}; white-space:nowrap; overflow:hidden; \
                            text-overflow:ellipsis; \
                            font-family:Fira Sans, DejaVu Sans, sans-serif;",
                    "{label}"
                }
            }
        }
    }
}

/// The waveform, as one filled shape mirrored about the centre line.
///
/// A polygon rather than per-column strokes: the skill's stroke trap
/// (resvg halves a stroke over its own path) and one shape instead of
/// hundreds. The viewBox is one unit per block and the `<svg>` stretches
/// to the item — `preserve_aspect_ratio: none`, the same move the fader's
/// stretch band makes.
#[component]
pub fn Waveform(amps: Vec<f32>, width: f32, top: f32, height: f32, colour: String) -> Element {
    let n = amps.len();
    // Above ~4 blocks per pixel the polygon outruns what the pixel can
    // show; fold blocks together so the shape stays a few hundred points.
    let max_points = (width as usize).clamp(64, 512);
    let folded: Vec<f32> = if n > max_points {
        let per = n as f32 / max_points as f32;
        (0..max_points)
            .map(|i| {
                let a = (i as f32 * per) as usize;
                let b = (((i + 1) as f32 * per) as usize).min(n);
                amps[a..b.max(a + 1)].iter().fold(0.0f32, |m, v| m.max(*v))
            })
            .collect()
    } else {
        amps
    };
    let count = folded.len().max(2);
    let span = count - 1;

    // Top edge left to right, bottom edge right to left — one closed shape.
    let mut d = String::with_capacity(count * 24);
    for (i, a) in folded.iter().enumerate() {
        let y = 1.0 - (*a).clamp(0.0, 1.0) * 0.96;
        d.push_str(if i == 0 { "M" } else { "L" });
        d.push_str(&format!(" {i} {y:.3} "));
    }
    for (i, a) in folded.iter().enumerate().rev() {
        let y = 1.0 + (*a).clamp(0.0, 1.0) * 0.96;
        d.push_str(&format!("L {i} {y:.3} "));
    }
    d.push('Z');

    rsx! {
        svg {
            style: "position:absolute; left:0; top:{top}px; display:block; \
                    width:{width}px; height:{height}px;",
            width: "{width}",
            height: "{height}",
            preserve_aspect_ratio: "none",
            view_box: "0 0 {span} 2",
            xmlns: "http://www.w3.org/2000/svg",
            path { d: "{d}", fill: "{colour}", fill_opacity: "0.85" }
            // The centre line survives silence — an empty bar still reads
            // as audio, which is REAPER's behaviour.
            rect { x: "0", y: "0.985", width: "{span}", height: "0.03",
                   fill: "{colour}", fill_opacity: "0.5" }
        }
    }
}

/// The note preview — pitch rows across the item, REAPER's MIDI block.
///
/// Rows are normalised to the notes the item actually has, padded a
/// semitone either side, so a two-note bassline does not draw as two
/// hairlines at the bottom of a 128-row grid.
#[component]
pub fn NoteRoll(
    notes: Vec<NotePreview>,
    length: f32,
    width: f32,
    top: f32,
    height: f32,
    colour: String,
) -> Element {
    let lo = notes
        .iter()
        .map(|n| n.pitch)
        .min()
        .unwrap_or(60)
        .saturating_sub(1);
    let hi = notes
        .iter()
        .map(|n| n.pitch)
        .max()
        .unwrap_or(72)
        .saturating_add(1);
    let rows = (hi - lo + 1).max(2) as f32;
    let secs = length.max(0.001);

    rsx! {
        svg {
            style: "position:absolute; left:0; top:{top}px; display:block; \
                    width:{width}px; height:{height}px;",
            width: "{width}",
            height: "{height}",
            preserve_aspect_ratio: "none",
            view_box: "0 0 {secs} {rows}",
            xmlns: "http://www.w3.org/2000/svg",
            for (i, n) in notes.iter().enumerate() {
                rect {
                    key: "{i}",
                    x: "{n.start}",
                    // Row 0 at the top is the highest pitch.
                    y: "{(hi - n.pitch) as f32 + 0.15}",
                    width: "{n.length.max(secs * 0.004)}",
                    height: "0.7",
                    fill: "{colour}",
                    fill_opacity: "0.9",
                }
            }
        }
    }
}

/// One envelope drawn over its lane: the line, and a square at each point.
///
/// Drawn in pixel space — the `<svg>`'s viewBox is its own box, no
/// stretching — so the stroke stays a true 1.2px whatever the zoom, which
/// a `preserve_aspect_ratio: none` band cannot promise. Only the value
/// line for now: REAPER's fill-free look. Square shapes step, everything
/// else draws linear until curves matter.
#[component]
fn EnvelopeLane(
    envelope: EnvelopePreview,
    top: f32,
    height: f32,
    width: f32,
    pixels_per_second: f32,
) -> Element {
    let t = daw_theme::Theme::default();
    let ink = envelope.colour(&t).css();
    // Two rows of margin so a full-scale envelope does not sit on the
    // lane divider.
    let (pad, h) = (2.0, height - 4.0);
    let y_of = |v: f32| pad + (1.0 - v.clamp(0.0, 1.0)) * h;

    // Linear between points, with the first value held back to the
    // origin and the last held to the end of the view — REAPER's rule.
    // Shape (square, bezier) refines this later; the *hold* semantics are
    // the part a viewer misreads if absent.
    let mut d = String::new();
    let mut last_y = None;
    for (i, (time, value)) in envelope.points.iter().enumerate() {
        let x = time * pixels_per_second;
        let y = y_of(*value);
        if i == 0 {
            d.push_str(&format!("M 0 {y:.2} L {x:.2} {y:.2} "));
        } else {
            d.push_str(&format!("L {x:.2} {y:.2} "));
        }
        last_y = Some(y);
    }
    if let Some(y) = last_y {
        d.push_str(&format!("L {width:.2} {y:.2}"));
    }

    rsx! {
        svg {
            style: "position:absolute; left:0; top:{top}px; display:block; \
                    width:{width}px; height:{height}px; pointer-events:none;",
            width: "{width}",
            height: "{height}",
            view_box: "0 0 {width} {height}",
            xmlns: "http://www.w3.org/2000/svg",
            path {
                d: "{d}",
                fill: "none",
                stroke: "{ink}",
                stroke_width: "1.2",
                stroke_opacity: "0.9",
            }
            for (i, (time, value)) in envelope.points.iter().enumerate() {
                rect {
                    key: "{i}",
                    x: "{time * pixels_per_second - 2.0}",
                    y: "{y_of(*value) - 2.0}",
                    width: "4",
                    height: "4",
                    fill: "{ink}",
                }
            }
        }
    }
}

/// The grab strip at the foot of an envelope lane.
///
/// REAPER resizes a lane by dragging its bottom edge, and the lane's
/// height *is* the envcp row's height (`plan_rows` states that once), so
/// one drag moves both panels. Relative like every other drag in this
/// crate — grabbing the edge must not jump the lane to the pointer.
///
/// The write goes out through `EnvelopeHandle::show_in_lane`, which
/// clamps to the theme's floor host-side; the local clamp is so the
/// preview never draws below the floor mid-drag.
#[component]
fn LaneResizeHandle(
    height: f32,
    width: f32,
    top: f32,
    /// Called with the new height as the pointer moves, and once more on
    /// release — the caller decides which of those reaches the DAW.
    on_resize: EventHandler<f32>,
    /// Called on release with the settled height.
    #[props(default)]
    on_commit: Option<EventHandler<f32>>,
) -> Element {
    let mut dragging = use_signal(|| false);
    let mut grab_y = use_signal(|| 0.0f32);
    let mut grab_h = use_signal(|| 0.0f32);

    let press = move |e: MouseEvent| {
        dragging.set(true);
        grab_y.set(e.client_coordinates().y as f32);
        grab_h.set(height);
    };
    let drag = move |e: MouseEvent| {
        if !dragging() {
            return;
        }
        let dy = e.client_coordinates().y as f32 - grab_y();
        on_resize.call((grab_h() + dy).max(ENVELOPE_LANE_MIN_H));
    };
    let release = move |e: MouseEvent| {
        if dragging.replace(false) {
            if let Some(commit) = &on_commit {
                let dy = e.client_coordinates().y as f32 - grab_y();
                commit.call((grab_h() + dy).max(ENVELOPE_LANE_MIN_H));
            }
        }
    };

    rsx! {
        div {
            // Four rows straddling the lane's foot: thin enough not to
            // steal clicks from the automation above it, thick enough to
            // hit without aiming.
            style: "position:absolute; left:0; top:{top + height - 2.0}px; \
                    width:{width}px; height:4px; cursor:ns-resize; z-index:3;",
            onmousedown: press,
            onmousemove: drag,
            onmouseup: release,
            onmouseleave: move |_| { dragging.set(false); },
        }
    }
}

/// One automation item on an envelope lane.
///
/// REAPER's shape: a translucent block in the envelope's own colour with
/// a thin header carrying the name (and the pool mark — a pooled
/// instance edits its siblings, so it must be tellable), the envelope
/// segment drawn inside the body. The lane's underlying envelope shows
/// through around it, which is what makes an AI read as a *window onto*
/// automation rather than a second kind of media item.
#[component]
fn AutomationItemBlock(
    item: AutomationItemView,
    top: f32,
    height: f32,
    pixels_per_second: f32,
    colour: daw_theme::Color,
) -> Element {
    let left = item.start * pixels_per_second;
    let w = (item.length * pixels_per_second).max(8.0);
    let header_h = 10.0f32;
    let body_h = (height - header_h - 3.0).max(4.0);
    let ink = colour.css();
    let header_bg = colour.shade(-0.55).css();
    let name = if item.pooled {
        format!("∞ {}", item.name)
    } else {
        item.name.clone()
    };

    // The segment, in the body's pixel space.
    let mut d = String::new();
    for (i, (time, value)) in item.points.iter().enumerate() {
        let x = (time * pixels_per_second).min(w);
        let y = (1.0 - value.clamp(0.0, 1.0)) * (body_h - 4.0) + 2.0;
        d.push_str(if i == 0 { "M" } else { "L" });
        d.push_str(&format!(" {x:.2} {y:.2} "));
    }

    rsx! {
        div {
            style: "position:absolute; left:{left}px; top:{top + 1.0}px; \
                    width:{w}px; height:{height - 3.0}px; \
                    border:1px solid {ink}; border-radius:2px; \
                    background:rgba(0,0,0,0.25); overflow:hidden;",
            div {
                style: "height:{header_h}px; background:{header_bg}; \
                        font-size:7px; line-height:{header_h}px; padding:0 3px; \
                        color:{ink}; white-space:nowrap; overflow:hidden; \
                        font-family:Fira Sans, DejaVu Sans, sans-serif;",
                "{name}"
            }
            svg {
                style: "display:block;",
                width: "{w}", height: "{body_h}",
                view_box: "0 0 {w} {body_h}",
                xmlns: "http://www.w3.org/2000/svg",
                path {
                    d: "{d}",
                    fill: "none",
                    stroke: "{ink}",
                    stroke_width: "1.1",
                    stroke_opacity: "0.95",
                }
            }
        }
    }
}

/// The live arrangement view: [`ArrangePreview`] fed from the DAW.
///
/// Tracks arrive on the track stream through the shared store, like every
/// panel; items are polled, because there is no item `#[subscribe]` stream
/// yet. The poll waits on the connection with `futures_timer`, **not**
/// tokio — this panel runs inside REAPER on dioxus's own scheduler, where
/// a tokio timer is a non-unwinding panic that takes the host down (the
/// placeholder this replaces had exactly that bug).
#[component]
pub fn ArrangementView(
    /// The session workflow modes to show in `LeftToolbar`/
    /// `ModeIndicator` — see the module doc on why `daw-ui` doesn't
    /// define this list itself. Empty hides both.
    #[props(default)]
    modes: Vec<ModeOption>,
    /// The active mode's slug. Ignored (nothing highlights) if it
    /// doesn't match any entry in `modes`.
    #[props(default)]
    active_mode_slug: String,
    /// Fired with the clicked mode's slug — the caller owns the actual
    /// state (see the module doc on why `daw-ui` can't safely do this
    /// itself for a standalone, non-REAPER host).
    #[props(default)]
    on_mode_change: Option<EventHandler<String>>,
    /// Mode-specific toolbar actions, supplied by the caller — same
    /// reason `modes` is caller-supplied: `daw-ui` doesn't know what
    /// each session mode's toolbar should contain. Rendered in
    /// `TopToolbar` ahead of the always-present zoom actions.
    #[props(default)]
    top_actions: Vec<ToolbarAction>,
) -> Element {
    let store = use_track_store();
    use_daw_tracks(store);

    let mut items = use_signal(Vec::<Item>::new);
    let mut markers = use_signal(Vec::<daw_proto::Marker>::new);
    let mut regions = use_signal(Vec::<daw_proto::Region>::new);
    let mut previews = use_signal(HashMap::<String, ItemPreview>::new);
    let mut envelopes = use_signal(HashMap::<String, Vec<EnvelopePreview>>::new);
    let mut env_lanes = use_signal(HashMap::<String, Vec<EnvelopeLaneView>>::new);
    let mut connected = use_signal(|| false);
    let mut size = use_signal(|| Option::<(f32, f32)>::None);
    let mut play_position = use_signal(|| 0.0f32);
    let mut edit_position = use_signal(|| 0.0f32);
    let mut zoom = use_signal(|| PX_PER_SECOND);
    // Vertical zoom: a multiplier on the TCP/arrange shared row pitch —
    // REAPER's "track height" zoom. TCP and arrange read the same
    // `row_h` below, so they always zoom together (the same way they
    // already scroll together).
    let mut vzoom = use_signal(|| 1.0f32);
    let row_h = daw_theme_art::geometry::tcp::ROW_H * vzoom();
    // The keybind profile picker — real `profile.styx` data (name +
    // description) `reaper-input` also loads, from the SAME checked-in
    // config directory (see `profiles_root`).
    let profiles = use_signal(|| KeybindProfile::list(&profiles_root()));
    let mut active_profile = use_signal(|| "fasttrackstudio".to_string());

    // The real input engine, loaded from the active profile's ACTUAL
    // keybind data (`transport.styx`/`navigation.styx`/...) — the same
    // files `reaper-input` loads, via `input_keybinds::load_profile_keymap`.
    // `use_hook` (not `use_signal`): loaded once at mount, then only
    // refreshed explicitly on a profile switch (below) — reading the
    // profile's ~20 styx files on every render would be needless I/O at
    // the ~30Hz this component already re-renders for the play cursor.
    let input_handle = use_input_processor(use_hook(|| load_keymap("fasttrackstudio")));
    // Track selection (j/k) is a local UI concept — no DAW round trip:
    // REAPER's own "select track" is a much heavier operation (arm
    // state, TCP scroll, action context) this doesn't attempt to mirror.
    let mut selected_track = use_signal(|| Option::<usize>::None);

    // Play + edit cursor: one continuous ~30Hz stream carries both
    // (`PositionTick` — REAPER pushes edit-cursor position alongside the
    // playhead on every tick, they're independent while transport runs).
    // A separate, dedicated future from the 2-second items/envelopes poll
    // above — position needs to be smooth, not coarse.
    use_future(move || async move {
        let Some(project) = crate::controls::reach::connected_project().await else {
            return;
        };
        let guid = project.guid().to_string();
        let mut stream = project.transport().events();
        while let Ok(Some(ev)) = stream.recv().await {
            if let daw_proto::transport::TransportStreamEvent::Position(tick) = ev.get() {
                if tick.project_guid != guid {
                    continue;
                }
                if let Some(s) = tick.playhead.seconds() {
                    play_position.set(s as f32);
                }
                if let Some(s) = tick.edit_cursor.seconds() {
                    edit_position.set(s as f32);
                }
            }
        }
    });

    use_future(move || async move {
        let Some(project) = crate::controls::reach::connected_project().await else {
            return;
        };
        connected.set(true);
        loop {
            if let Ok(list) = project.items().all().await {
                // Fetch content previews for items that do not have one
                // yet — once per (guid, length), so an edit that changes
                // the take refetches and everything else stays cached.
                for item in &list {
                    let key = item.guid.clone();
                    if previews.read().contains_key(&key) {
                        continue;
                    }
                    if let Some(preview) = fetch_preview(&project, item).await {
                        previews.write().insert(key, preview);
                    }
                }
                items.set(list);
            }
            // Markers/regions — what the Organize toolbar's Count In/
            // =START/section/=END buttons actually write
            // (`session::keyflow::actions::dispatch`). Same poll cadence
            // as items; there's no live stream for these yet.
            if let Ok(list) = project.markers().all().await {
                markers.set(list);
            }
            if let Ok(list) = project.regions().all().await {
                regions.set(list);
            }
            // Visible envelopes, refreshed on the same cadence — an
            // automation pass in REAPER shows up within a poll. Only the
            // ones REAPER shows: `visible` is the arrange-view flag.
            if let Ok(track_list) = project.tracks().all().await {
                let mut overlaid = HashMap::new();
                let mut laned: HashMap<String, Vec<EnvelopeLaneView>> = HashMap::new();
                for track in &track_list {
                    let Ok(Some(handle)) = project.tracks().by_guid(&track.guid).await else {
                        continue;
                    };
                    let Ok(all) = handle.envelopes().all().await else {
                        continue;
                    };
                    let mut over = Vec::new();
                    let mut lanes = Vec::new();
                    for env in all.iter().filter(|e| e.visible) {
                        let Ok(Some(eh)) = handle.envelopes().by_type(env.envelope_type).await
                        else {
                            continue;
                        };
                        let Ok(points) = eh.points().await else {
                            continue;
                        };
                        let preview = EnvelopePreview {
                            name: env.name.clone(),
                            points: points
                                .iter()
                                .map(|p| (p.time.as_seconds() as f32, p.value as f32))
                                .collect(),
                        };
                        // The envelope's own choice decides where it is
                        // drawn — over the track, or in a lane of its own
                        // with its automation items.
                        if env.in_own_lane {
                            let mut automation_items = Vec::new();
                            if env.automation_item_count > 0 {
                                if let Ok(items) = eh.automation_items().await {
                                    for ai in &items {
                                        let pts = eh
                                            .automation_item_points(ai.index)
                                            .await
                                            .unwrap_or_default();
                                        automation_items.push(AutomationItemView {
                                            name: ai.name.clone(),
                                            start: ai.position.as_seconds() as f32,
                                            length: ai.length.as_seconds() as f32,
                                            // Every REAPER automation
                                            // item has a pool; only one
                                            // shared by siblings needs
                                            // the warning mark.
                                            pooled: items
                                                .iter()
                                                .filter(|o| o.pool_id == ai.pool_id)
                                                .count()
                                                > 1,
                                            points: pts
                                                .iter()
                                                .map(|p| {
                                                    (p.time.as_seconds() as f32, p.value as f32)
                                                })
                                                .collect(),
                                        });
                                    }
                                }
                            }
                            lanes.push(EnvelopeLaneView {
                                envelope: preview,
                                height: env.lane_height as f32,
                                automation_items,
                            });
                        } else if !preview.points.is_empty() {
                            over.push(preview);
                        }
                    }
                    if !over.is_empty() {
                        overlaid.insert(track.guid.clone(), over);
                    }
                    if !lanes.is_empty() {
                        laned.insert(track.guid.clone(), lanes);
                    }
                }
                envelopes.set(overlaid);
                env_lanes.set(laned);
            }
            futures_timer::Delay::new(std::time::Duration::from_secs(2)).await;
        }
    });

    if !*connected.read() {
        return rsx! {
            div {
                style: "height:100%; width:100%; display:flex; align-items:center; \
                        justify-content:center; font-size:12px; color:#7b7b7b;",
                "Waiting for DAW connection..."
            }
        };
    }

    // Project order from the store's own ordering — the same order the
    // meter frame is indexed by.
    let tracks: Vec<Track> = store
        .order()
        .iter()
        .filter_map(|guid| store.track(guid))
        .collect();
    let item_list = items.read().clone();
    // Drawn at the box the dock gives, measured on mount — the same move
    // the mixer makes, for the same reason.
    let (w, h) = size.read().unwrap_or((800.0, 600.0));

    // The TCP's spacer + row plan mirror `MainWindowPreview`'s "TCP |
    // arrangement" block exactly — one row list, two renderings, sharing
    // `plan_rows` so an envcp lane is exactly as tall on both sides.
    let env_lanes_snapshot = env_lanes.read().clone();
    let plan = plan_rows(&tracks, &env_lanes_snapshot, row_h);
    let depths = folder_depths(&tracks);
    let arrange_w = (w - ROW_W).max(0.0);

    // TCP and the arrange lanes are ONE scroll surface below (see the
    // module doc) — no separate scroll container to keep in sync, so
    // this computes the same content-canvas math `ArrangePreview` does
    // for its own (non-live, screenshot-only) scroll box.
    let pixels_per_second = zoom();
    let bar_secs = 240.0 / 120.0_f64; // 4/4 at 120bpm until the tempo map is wired
    let bar_px = bar_secs as f32 * pixels_per_second;
    let beat_px = bar_px / 4.0;
    let draw_beats = beat_px >= 12.0;
    let content_secs = item_list
        .iter()
        .map(|it| it.position.as_seconds() as f32 + it.length.as_seconds() as f32)
        .fold(0.0f32, f32::max)
        + arrange_w / pixels_per_second.max(1.0);
    let content_w = (content_secs * pixels_per_second).max(arrange_w);
    let content_h = plan
        .iter()
        .map(|(_, top, rh)| top + rh)
        .fold(0.0f32, f32::max)
        .max(h - RULER_H - 2.0 * MARKER_BAND_H);
    let bars = (content_w / bar_px).ceil() as usize + 1;

    let theme = daw_theme::Theme::default();
    let ruler_bg = theme.chrome.surface_sunken.shade(-0.12).css();
    let ruler_ink = theme.chrome.text_dim.css();
    let rule = theme.chrome.surface_sunken.shade(-0.25).css();
    let bar_line = theme.chrome.surface_sunken.shade(-0.18).css();

    let zoom_actions = vec![
        ToolbarAction {
            id: "zoom-out".to_string(),
            label: "Zoom out".to_string(),
            glyph: "\u{2212}".to_string(),
            active: false,
            color: None,
            on_click: EventHandler::new(move |_| zoom.set((zoom() * 0.8).max(2.0))),
        },
        ToolbarAction {
            id: "zoom-in".to_string(),
            label: "Zoom in".to_string(),
            glyph: "+".to_string(),
            active: false,
            color: None,
            on_click: EventHandler::new(move |_| zoom.set((zoom() * 1.25).min(4000.0))),
        },
        ToolbarAction {
            id: "zoom-reset".to_string(),
            label: "Reset".to_string(),
            glyph: "\u{21bb}".to_string(),
            active: false,
            color: None,
            on_click: EventHandler::new(move |_| zoom.set(PX_PER_SECOND)),
        },
        ToolbarAction {
            id: "vzoom-out".to_string(),
            label: "Row \u{2212}".to_string(),
            glyph: String::new(),
            active: false,
            color: None,
            on_click: EventHandler::new(move |_| vzoom.set((vzoom() * 0.8).max(0.4))),
        },
        ToolbarAction {
            id: "vzoom-in".to_string(),
            label: "Row +".to_string(),
            glyph: String::new(),
            active: false,
            color: None,
            on_click: EventHandler::new(move |_| vzoom.set((vzoom() * 1.25).min(3.0))),
        },
        ToolbarAction {
            id: "vzoom-reset".to_string(),
            label: "Row \u{21bb}".to_string(),
            glyph: String::new(),
            active: false,
            color: None,
            on_click: EventHandler::new(move |_| vzoom.set(1.0)),
        },
    ];

    // Real keybind dispatch: `input_handle` matched the keydown against
    // the active profile's ACTUAL `transport.styx`/`navigation.styx`
    // bindings (space = play/stop, h/l = edit cursor by measure, j/k =
    // track select, enter = go to start — the same keys `reaper-input`
    // binds, driving them here through the backend-agnostic
    // `daw_control` transport/track traits instead of a REAPER action
    // id). Unrecognised/unsupported action ids are silently ignored —
    // most of a REAPER profile's ~150 actions (SWS extension commands,
    // dialogs, ...) have no standalone equivalent to dispatch to yet.
    let track_count = tracks.len();
    let keydown_input_handle = input_handle.clone();
    let handle_keydown = move |evt: KeyboardEvent| {
        for cmd in keydown_input_handle.handle_key(&evt) {
            let InputCommand::Action(id) = cmd else {
                continue;
            };
            // Mode switching: `<A-1>`…`<A-9>`/`<A-0>` and the `<A-m>` letter
            // tree both resolve (via `modes.styx`) to one of REAPER's
            // registered `_FTS_SESSION_MODE_<NAME>` actions — the same
            // naming `session_proto::mode::ModeActions` registers under.
            // The suffix lowercased IS the mode's slug
            // (`session::modes::Mode::slug()`), so no REAPER-specific
            // translation table is needed the way the transport ids
            // below require one.
            if let Some(slug) = id.as_str().strip_prefix("_FTS_SESSION_MODE_") {
                if let Some(handler) = &on_mode_change {
                    handler.call(slug.to_ascii_lowercase());
                }
                continue;
            }
            match id.as_str() {
                "40285" if track_count > 0 => {
                    // Select next track.
                    selected_track.set(Some(match selected_track() {
                        Some(i) if i + 1 < track_count => i + 1,
                        Some(i) => i,
                        None => 0,
                    }));
                }
                "40286" if track_count > 0 => {
                    // Select previous track.
                    selected_track.set(Some(match selected_track() {
                        Some(i) if i > 0 => i - 1,
                        other => other.unwrap_or(0),
                    }));
                }
                other => {
                    spawn(execute_transport_action(other.to_string()));
                }
            };
        }
    };

    rsx! {
        div {
            style: "height:100%; width:100%; outline:none; overflow:hidden; display:flex; flex-direction:column;",
            // Focusable so the real keybind dispatch above actually
            // receives key events — a plain `div` never does.
            tabindex: "0",
            onkeydown: handle_keydown,
            // `onmounted` gives the initial box; `onresize` (a real
            // ResizeObserver under the hood) is what keeps it current —
            // without it the arrange area stayed pinned to whatever size
            // it happened to be when this component first mounted, never
            // following the window/dock afterwards.
            onresize: move |evt| {
                if let Ok(size_box) = evt.get_content_box_size() {
                    if size_box.width > 0.0 && size_box.height > 0.0 {
                        size.set(Some((size_box.width as f32, size_box.height as f32)));
                    }
                }
            },
            onmounted: move |evt| {
                spawn(async move {
                    if let Ok(rect) = evt.get_client_rect().await {
                        if rect.size.width > 0.0 && rect.size.height > 0.0 {
                            size.set(Some((rect.size.width as f32, rect.size.height as f32)));
                        }
                    }
                });
            },
            // Mode dropdown (shows + changes the active mode) + the zoom
            // actions + the keybind profile picker, sharing one header row.
            div {
                style: "display:flex; flex-direction:row; align-items:center; \
                        justify-content:space-between; gap:8px; \
                        background:{daw_theme::Theme::default().chrome.surface.css()}; \
                        border-bottom:1px solid {daw_theme::Theme::default().chrome.surface_sunken.shade(-0.1).css()};",
                div { style: "display:flex; align-items:center; gap:8px; padding:4px 0 4px 6px;",
                    if !modes.is_empty() {
                        ModeDropdown {
                            modes: modes.clone(),
                            active_slug: active_mode_slug.clone(),
                            on_mode_change: move |slug: String| {
                                if let Some(handler) = &on_mode_change {
                                    handler.call(slug);
                                }
                            },
                        }
                    }
                }
                TopToolbar {
                    actions: top_actions.into_iter().chain(zoom_actions).collect::<Vec<_>>(),
                }
                div { style: "padding:4px 6px 4px 0;",
                    ProfilePicker {
                        profiles: profiles(),
                        active_slug: active_profile(),
                        on_select: move |slug: String| {
                            input_handle.reload_config(load_keymap(&slug));
                            active_profile.set(slug);
                        },
                    }
                }
            }
            div {
                style: "flex:1 1 0; min-height:0; display:flex;",
                // ── TCP + arrange lanes: ONE scroll surface, not two kept
                // in sync. A previous version scrolled the lanes natively
                // and mirrored the position into the TCP column via a
                // signal + transform — every scroll event forced a
                // re-render of this whole component to update that
                // transform, which is exactly the kind of full-tree churn
                // that made the view laggy (the same class of bug the
                // ~30Hz position cursor had — see `PositionCursor`'s doc).
                // There is no signal, no mirroring, and so no lag to fix:
                // the TCP column (`position:sticky; left:0`) and the
                // ruler (`position:sticky; top:0`, nested so its own
                // corner patch also sticks left) are just ordinary
                // content inside the ONE div below that actually scrolls.
                // The browser's compositor moves them together in the
                // same paint, the way a frozen pane in a spreadsheet
                // does — there is no code path where they could drift
                // apart.
                div {
                    style: "position:relative; flex:1 1 0; min-width:0; \
                            overflow:scroll; cursor:default;",
                    // Wheel zoom — `zoom.styx`'s real FastTrackStudio scheme:
                    // Alt+wheel zooms vertically (row height), Shift+Alt+wheel
                    // zooms horizontally (timeline), Ctrl+Alt+wheel zooms
                    // both. Plain wheel is left alone — that's the native
                    // scroll this whole view is built on (see the module
                    // doc), not something to intercept.
                    onwheel: move |evt: WheelEvent| {
                        let m = evt.modifiers();
                        if !m.alt() {
                            return;
                        }
                        evt.prevent_default();
                        let dy = evt.delta().strip_units().y;
                        let factor = if dy < 0.0 { 1.1 } else { 0.9 };
                        if m.ctrl() {
                            zoom.set((zoom() * factor).clamp(2.0, 4000.0));
                            vzoom.set((vzoom() * factor).clamp(0.4, 3.0));
                        } else if m.shift() {
                            zoom.set((zoom() * factor).clamp(2.0, 4000.0));
                        } else {
                            vzoom.set((vzoom() * factor).clamp(0.4, 3.0));
                        }
                    },
                    div {
                        style: "position:relative; width:{ROW_W + content_w}px; \
                                height:{RULER_H + 2.0 * MARKER_BAND_H + content_h}px;",

                        // Ruler row — sticky to the top; scrolls
                        // horizontally with everything else.
                        div {
                            style: "display:flex; flex-direction:row; position:sticky; \
                                    top:0; z-index:15; height:{RULER_H}px; \
                                    width:{ROW_W + content_w}px; background:{ruler_bg}; \
                                    border-bottom:1px solid {rule};",
                            // The corner: sticky to the left WITHIN the
                            // already-sticky-top ruler, so it alone stays
                            // put in both directions — the one patch that
                            // needs to, since it sits over the TCP column.
                            div {
                                style: "position:sticky; left:0; z-index:2; flex:0 0 auto; \
                                        width:{ROW_W}px; height:{RULER_H}px; \
                                        background:{ruler_bg};",
                            }
                            div {
                                style: "position:relative; flex:1 1 auto; overflow:hidden; cursor:text;",
                                // Plain click moves the edit cursor — REAPER's
                                // own default for `MM_CTX_RULER_CLK` with no
                                // modifier held (the modified variants in
                                // `mouse-profile.styx` are alternates on top
                                // of this base behavior). `element_coordinates`
                                // is already relative to this box's own
                                // origin, which is the timeline's zero — no
                                // scroll-offset math needed, native scroll
                                // already accounts for it.
                                onclick: move |evt: MouseEvent| {
                                    let x = evt.element_coordinates().x as f32;
                                    let seconds = (x / pixels_per_second).max(0.0) as f64;
                                    spawn(seek_to_seconds(seconds));
                                },
                                for bar in 0..bars {
                                    div {
                                        key: "r{bar}",
                                        style: "position:absolute; left:{bar as f32 * bar_px}px; top:0; \
                                                height:{RULER_H}px; border-left:1px solid {bar_line}; \
                                                padding-left:4px; font-size:9px; \
                                                line-height:{RULER_H}px; color:{ruler_ink}; \
                                                font-family:Fira Sans, DejaVu Sans, sans-serif; \
                                                pointer-events:none;",
                                        "{bar + 1}"
                                    }
                                }
                            }
                        }

                        // Regions lane, then Markers lane — REAPER's own
                        // default ruler layout (two fixed lanes above the
                        // track area; `Marker::lane`/`Region::lane` only
                        // add MORE lanes past v7.62, out of scope here).
                        // What the Organize toolbar's Count In/=START/
                        // section/=END buttons actually write
                        // (`session::keyflow::actions::dispatch` against
                        // the real `daw-standalone` backend, not REAPER).
                        // Both sticky to the top just below the ruler,
                        // same nested-sticky-corner trick, so the whole
                        // stack reads as one frozen header while still
                        // scrolling horizontally with the timeline.
                        //
                        // Clicking either lane's empty space also moves
                        // the edit cursor — same behavior as the ruler
                        // itself (REAPER's `MM_CTX_MARKERLANES` has no
                        // unmodified override, so the base ruler-click
                        // behavior applies there too).
                        for (lane_index, label, lane_top) in [
                            ("regions", "Regions", RULER_H),
                            ("markers", "Markers", RULER_H + MARKER_BAND_H),
                        ] {
                            {
                                let lane_index = lane_index;
                                rsx! {
                                div {
                                    key: "{lane_index}",
                                    style: "display:flex; flex-direction:row; position:sticky; \
                                            top:{lane_top}px; z-index:14; height:{MARKER_BAND_H}px; \
                                            width:{ROW_W + content_w}px; \
                                            background:{daw_theme::Theme::default().chrome.surface_sunken.css()}; \
                                            border-bottom:1px solid {rule};",
                                    div {
                                        style: "position:sticky; left:0; z-index:2; flex:0 0 auto; \
                                                width:{ROW_W}px; height:{MARKER_BAND_H}px; \
                                                background:{daw_theme::Theme::default().chrome.surface_sunken.css()}; \
                                                font-size:9px; color:{ruler_ink}; display:flex; \
                                                align-items:center; padding-left:4px; \
                                                font-family:Fira Sans, DejaVu Sans, sans-serif;",
                                        "{label}"
                                    }
                                    div {
                                        style: "position:relative; flex:1 1 auto; overflow:hidden; cursor:text;",
                                        onclick: move |evt: MouseEvent| {
                                            let x = evt.element_coordinates().x as f32;
                                            let seconds = (x / pixels_per_second).max(0.0) as f64;
                                            spawn(seek_to_seconds(seconds));
                                        },
                                        if lane_index == "regions" {
                                            for region in regions.read().iter() {
                                                {
                                                    let start = region.time_range.start.seconds().unwrap_or(0.0) as f32;
                                                    let end = region.time_range.end.seconds().unwrap_or(0.0) as f32;
                                                    let left = start * pixels_per_second;
                                                    let w = ((end - start) * pixels_per_second).max(2.0);
                                                    let bg = region
                                                        .color
                                                        .map(|c| format!("#{:06X}", c & 0xFF_FFFF))
                                                        .unwrap_or_else(|| "#6b7280".to_string());
                                                    rsx! {
                                                        div {
                                                            key: "region-{region.id:?}",
                                                            title: "{region.name}",
                                                            style: "position:absolute; left:{left}px; top:2px; \
                                                                    width:{w}px; height:{MARKER_BAND_H - 4.0}px; \
                                                                    background:{bg}; border-radius:2px; \
                                                                    overflow:hidden; white-space:nowrap; \
                                                                    font-size:9px; font-weight:600; color:#0a0a0a; \
                                                                    padding:0 4px; line-height:{MARKER_BAND_H - 4.0}px; \
                                                                    font-family:Fira Sans, DejaVu Sans, sans-serif; \
                                                                    pointer-events:none;",
                                                            "{region.name}"
                                                        }
                                                    }
                                                }
                                            }
                                        } else {
                                            for marker in markers.read().iter() {
                                                {
                                                    let pos = marker.position.seconds().unwrap_or(0.0) as f32;
                                                    let left = pos * pixels_per_second;
                                                    let bg = marker
                                                        .color
                                                        .map(|c| format!("#{:06X}", c & 0xFF_FFFF))
                                                        .unwrap_or_else(|| "#737373".to_string());
                                                    rsx! {
                                                        div {
                                                            key: "marker-{marker.id:?}",
                                                            title: "{marker.name}",
                                                            style: "position:absolute; left:{left}px; top:2px; \
                                                                    max-width:120px; height:{MARKER_BAND_H - 4.0}px; \
                                                                    background:{bg}; border-radius:2px; \
                                                                    overflow:hidden; white-space:nowrap; \
                                                                    font-size:9px; font-weight:600; color:#ffffff; \
                                                                    padding:0 4px; line-height:{MARKER_BAND_H - 4.0}px; \
                                                                    font-family:Fira Sans, DejaVu Sans, sans-serif; \
                                                                    pointer-events:none;",
                                                            "{marker.name}"
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

                        // TCP — sticky to the left; scrolls vertically
                        // with everything else (it's ordinary flow
                        // content directly below the ruler, not absolute
                        // — that's what lets `position:sticky` work).
                        div {
                            style: "position:sticky; left:0; z-index:10; width:{ROW_W}px; \
                                    background:{daw_theme::Theme::default().chrome.surface.css()};",
                            for (kind , _ , row_height) in plan.iter().copied() {
                                match kind {
                                    ArrangeRowKind::Track(i) => rsx! {
                                        TrackRow {
                                            key: "{tracks[i].guid}",
                                            track: tracks[i].clone(),
                                            index: i as u32,
                                            height: row_h,
                                            depth: depths[i],
                                            selected: selected_track() == Some(i),
                                        }
                                    },
                                    ArrangeRowKind::EnvelopeLane { track, lane } => {
                                        let view = &env_lanes_snapshot[&tracks[track].guid][lane];
                                        let fx_param = view.envelope.name.contains('/');
                                        rsx! {
                                            EnvcpRow {
                                                key: "e{track}-{lane}",
                                                name: view.envelope.name.clone(),
                                                height: row_height,
                                                fx_param,
                                                armed: true,
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Arrange content — absolute, laid out to the
                        // right of the TCP and below the ruler, in the
                        // SAME scroll box as both.
                        div {
                            style: "position:absolute; left:{ROW_W}px; top:{RULER_H + 2.0 * MARKER_BAND_H}px;",
                            ArrangeCanvas {
                                tracks,
                                items: item_list,
                                previews: previews.read().clone(),
                                envelopes: envelopes.read().clone(),
                                env_lanes: env_lanes_snapshot,
                                width: content_w,
                                height: content_h,
                                pixels_per_second: zoom(),
                                row_h,
                                bar_px,
                                beat_px,
                                draw_beats,
                                bars,
                                play_position,
                                edit_position,
                                // Optimistic: flip the local guid(s) immediately
                                // (no round-trip stutter, same reasoning as the
                                // lane-resize drag below) and write the real
                                // selection through to the DAW.
                                on_item_click: move |guid: Option<String>| {
                                    items.write().iter_mut().for_each(|it| {
                                        it.selected = Some(&it.guid) == guid.as_ref();
                                    });
                                    spawn(select_only_item(guid));
                                },
                                // A drag moves the local lane immediately and
                                // writes the settled height through to the
                                // DAW — the same optimistic shape the faders
                                // use, because a lane that waits on a round
                                // trip stutters under the pointer.
                                on_lane_resize: move |(guid, lane, height): (String, usize, f32)| {
                                    if let Some(lanes) = env_lanes.write().get_mut(&guid) {
                                        if let Some(view) = lanes.get_mut(lane) {
                                            view.height = height;
                                        }
                                    }
                                    spawn(async move {
                                        let Some(project) = crate::controls::reach::connected_project().await
                                        else {
                                            return;
                                        };
                                        let Ok(Some(track)) = project.tracks().by_guid(&guid).await else {
                                            return;
                                        };
                                        let Ok(all) = track.envelopes().all().await else { return };
                                        // The lane index is a position among the
                                        // envelopes that *have* lanes, which is what the
                                        // view was built from.
                                        let Some(env) = all.iter().filter(|e| e.in_own_lane).nth(lane) else {
                                            return;
                                        };
                                        if let Ok(Some(handle)) =
                                            track.envelopes().by_type(env.envelope_type).await
                                        {
                                            let _ = handle.show_in_lane(height.round() as u32).await;
                                        }
                                    });
                                },
                            }
                        }
                    }
                }
                RightToolbar { actions: vec![] }
            }
        }
    }
}

/// Execute one REAPER action id from the active keybind profile against
/// the connected project's backend-agnostic `daw_control` transport —
/// the small, real subset of a profile's ~150 actions that have a
/// standalone equivalent. Everything else is a silent no-op (there is
/// no "unsupported action" UI yet, matching how an unbound key already
/// does nothing).
async fn execute_transport_action(action_id: String) {
    let Some(project) = crate::controls::reach::connected_project().await else {
        return;
    };
    let transport = project.transport();
    let result = match action_id.as_str() {
        // Play/Stop.
        "40044" => transport.play_stop().await,
        // Play/Pause.
        "40073" => transport.play_pause().await,
        // Go to start of project.
        "40042" | "40161" => transport.goto_start().await,
        // Toggle recording.
        "1013" => transport.toggle_recording().await,
        // Move edit cursor to previous/next measure — no dedicated
        // "by measure" verb on the generic transport, so this is
        // computed from the current tempo (4/4 assumed, same
        // simplification `ArrangePreview`'s ruler already makes).
        "40838" => move_edit_cursor_by_measure(&transport, -1.0).await,
        "40837" => move_edit_cursor_by_measure(&transport, 1.0).await,
        _ => return,
    };
    if let Err(e) = result {
        tracing::debug!(action = %action_id, error = %e, "transport action failed");
    }
}

/// Move the edit cursor to an exact position — a plain click on the
/// ruler in REAPER's own default mouse map (`MM_CTX_RULER_CLK` with no
/// modifier held; the modified variants in `mouse-profile.styx` are all
/// alternate behaviors layered on top of this base one).
async fn seek_to_seconds(seconds: f64) {
    let Some(project) = crate::controls::reach::connected_project().await else {
        return;
    };
    if let Err(e) = project.transport().set_position(seconds.max(0.0)).await {
        tracing::debug!(error = %e, seconds, "ruler click seek failed");
    }
}

/// Select exactly `guid` (deselecting everything else), or clear the
/// selection entirely when `guid` is `None` — REAPER's own
/// `MM_CTX_ITEMLOWER_CLK` ("select item") / `MM_CTX_TRACK_CLK`
/// ("deselect all items") defaults for a plain click.
async fn select_only_item(guid: Option<String>) {
    let Some(project) = crate::controls::reach::connected_project().await else {
        return;
    };
    let items = project.items();
    if let Err(e) = items.deselect_all().await {
        tracing::debug!(error = %e, "deselect_all failed");
        return;
    }
    let Some(guid) = guid else { return };
    match items.by_guid(&guid).await {
        Ok(Some(handle)) => {
            if let Err(e) = handle.select().await {
                tracing::debug!(error = %e, guid, "select item failed");
            }
        }
        Ok(None) => {}
        Err(e) => tracing::debug!(error = %e, guid, "by_guid failed"),
    }
}

async fn move_edit_cursor_by_measure(
    transport: &daw_control::Transport,
    measures: f64,
) -> daw_control::Result<()> {
    let bpm = transport.get_tempo().await.unwrap_or(120.0).max(1.0);
    let bar_secs = measures * 240.0 / bpm;
    let pos = transport.get_position().await.unwrap_or(0.0);
    transport.set_position((pos + bar_secs).max(0.0)).await
}

/// One item's content preview, from whichever kind of take it holds.
///
/// Audio goes through [`TakeHandle::peaks`] — under REAPER that is
/// `PCM_source::GetPeaks`, served from the `.reapeaks` cache REAPER has
/// already built for its own arrange view, so no media is decoded. MIDI
/// reads the take's notes and converts PPQ to seconds at the project's
/// tempo. Anything else (empty, video, a backend with no peak store)
/// yields `None` and the item draws its plain block.
///
/// [`TakeHandle::peaks`]: daw_control::TakeHandle::peaks
pub(crate) async fn fetch_preview(
    project: &daw_control::Project,
    item: &Item,
) -> Option<ItemPreview> {
    let handle = project.items().by_guid(&item.guid).await.ok()??;
    let take = handle.active_take();
    let info = take.info().await.ok()?;
    match info.source_type {
        daw_proto::SourceType::Audio => {
            // ~86 peaks a second at 44.1k: enough for any zoom the panel
            // draws, small enough to ship for every item in a project.
            let data = take.peaks(512).await.ok()?;
            let amps = waveform_from_peaks(&data);
            (!amps.is_empty()).then_some(ItemPreview::Waveform(amps))
        }
        daw_proto::SourceType::Midi => {
            let notes = handle.active_take().midi().notes().await.ok()?;
            // Quarter-notes to seconds at the tempo the item starts in.
            // One tempo, not the map — the preview is a thumbnail.
            let bpm = project
                .tempo_map()
                .tempo_at(item.position.as_seconds())
                .await
                .unwrap_or(120.0);
            let spq = (60.0 / bpm.max(1.0)) as f32;
            let notes: Vec<NotePreview> = notes
                .iter()
                .map(|n| NotePreview {
                    pitch: n.pitch,
                    start: n.start_ppq as f32 * spq,
                    length: n.length_ppq as f32 * spq,
                })
                .collect();
            (!notes.is_empty()).then_some(ItemPreview::Notes(notes))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(guid: &str) -> Track {
        Track {
            guid: guid.into(),
            ..Default::default()
        }
    }

    /// A resize drag reports heights clamped to the envcp floor, and
    /// the plan then lays out at exactly what the drag reported — the
    /// two clamps agree, so a lane dragged past the stop does not jump
    /// when the next frame plans it.
    #[test]
    fn a_drag_past_the_stop_settles_at_the_floor() {
        let dragged = |grab_h: f32, dy: f32| (grab_h + dy).max(ENVELOPE_LANE_MIN_H);

        // Dragged well past the bottom stop.
        let settled = dragged(40.0, -80.0);
        assert_eq!(settled, ENVELOPE_LANE_MIN_H);

        // And the plan draws it there rather than clamping again to
        // something else.
        let tracks = vec![track("a")];
        let lanes = HashMap::from([(
            "a".to_string(),
            vec![EnvelopeLaneView {
                envelope: EnvelopePreview {
                    name: "Volume".into(),
                    points: vec![],
                },
                height: settled,
                automation_items: Vec::new(),
            }],
        )]);
        let plan = plan_rows(&tracks, &lanes, 70.0);
        assert_eq!(plan[1].2, ENVELOPE_LANE_MIN_H);
    }

    /// The vertical plan interleaves lanes under their tracks, respects
    /// the envcp floor, and accounts every divider — the numbers both the
    /// arrange and the TCP column lay out by.
    #[test]
    fn the_plan_interleaves_and_holds_the_floor() {
        let tracks = vec![track("a"), track("b")];
        let lane = |h: f32| EnvelopeLaneView {
            envelope: EnvelopePreview {
                name: "Volume".into(),
                points: vec![],
            },
            height: h,
            automation_items: Vec::new(),
        };
        let lanes = HashMap::from([
            // One healthy lane and one below the floor.
            ("a".to_string(), vec![lane(40.0), lane(10.0)]),
        ]);

        let plan = plan_rows(&tracks, &lanes, 70.0);
        assert_eq!(plan.len(), 4);
        assert_eq!(plan[0], (ArrangeRowKind::Track(0), 0.0, 70.0));
        assert_eq!(
            plan[1],
            (
                ArrangeRowKind::EnvelopeLane { track: 0, lane: 0 },
                71.0,
                40.0
            )
        );
        // The 10-tall lane is held at the envcp floor.
        assert_eq!(
            plan[2],
            (
                ArrangeRowKind::EnvelopeLane { track: 0, lane: 1 },
                112.0,
                ENVELOPE_LANE_MIN_H
            )
        );
        // And track b starts below it, divider counted.
        assert_eq!(plan[3], (ArrangeRowKind::Track(1), 140.0, 70.0));
    }
}
