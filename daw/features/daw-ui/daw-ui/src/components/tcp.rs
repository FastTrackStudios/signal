//! The track control panel — REAPER's TCP, as Dioxus.
//!
//! The mixer strip's sibling, and deliberately built the same way: the
//! numbers are REAPER's, taken from `daw-theme-art`'s panel sheet, which
//! composes the same components at the same coordinates for the exporter.
//! Two renderings of one layout is the point — if they drift, the sheet is
//! the thing that is wrong.
//!
//! # It is a row, not a column
//!
//! Everything the mixer strip learned still applies, but rotated. The
//! track colour tints the *whole* row rather than a band inside it; the
//! controls sit on two rows of 17-tall fields; and mute and solo live
//! outside the tint in their own 44-wide gutter, which is why the row is
//! 340 wide and the tint only 296.
//!
//! # Layout-critical values are inline
//!
//! Same rule, same reason: every window in this tree embeds its stylesheet
//! as a static string, and a panel that mounts before the sheet arrives —
//! or a test that mounts without one — must still lay out. Tailwind is the
//! additive layer. The mixer strip spent an afternoon learning this the
//! hard way; see `components::mixer`.

use daw_proto::Track;
use daw_theme_art::dress::Panel;
use daw_theme_art::vector_controls as art;

use crate::controls::{
    Caret, EnvelopeButton, FxButton, IoButton, MuteButton, PanKnob, PhaseButton, RecordArmButton,
    SoloButton, TrackMeter, record_input_name, use_live_track, use_track_store,
};
use crate::prelude::*;

// The row's geometry — measured off REAPER itself rather than off the
// panel sheet — lives in `daw_theme_art::geometry::tcp`, one home for the
// panel's facts. The measurement notes travel with the numbers.
use daw_theme_art::geometry::tcp::{
    COLUMN_RULE_X, FIELD_H, FX_IN_X, GUTTER_BUTTON_X, GUTTER_W, LANES_FROM_FLOOR, LANES_HIDE_H,
    METER_W as TCP_METER_W, METER_X as TCP_METER_X, NAME_FIELD_H, NAME_FIELD_W, NAME_FIELD_X,
    PAN_KNOB_X, PHASE_FROM_FLOOR, PHASE_HIDE_H, ROUTING_X, ROW_H, ROW_ONE, ROW_TWO, ROW_W,
    SOLO_TOP, TINT_W, VOLUME_KNOB_X,
};

/// The panel's own grey — the same one the mixer strip's body and name
/// plate use. A measured token, not a local hex, so a re-palette reaches
/// it (#240).
use daw_theme::defaults::STRIP_BODY as BODY_GREY;

/// One track's row in the panel.
///
/// Public so a test, a playground or a design review can photograph a row
/// without a backend behind it — the same reason `ChannelStripPreview`
/// exists on the mixer side.
#[component]
pub fn TrackRow(
    track: Track,
    #[props(default)] index: u32,
    /// The row's height. REAPER's track panel grows with the track, and
    /// two of its controls only exist above a height — phase and the
    /// fixed-lanes button — so a row that could not grow could not show
    /// them at all.
    #[props(default = ROW_H)]
    height: f32,
    /// REAPER-style folder nesting depth (0 = top level) — see
    /// `arrangement_view::folder_depths`. Shifts the pin/folder-glyph/
    /// name-field cluster right and narrows the name field by the same
    /// amount, so deeper tracks read as nested without moving the knobs/
    /// meter/mute/solo, which stay at their measured absolute positions.
    #[props(default)]
    depth: u32,
    /// Local j/k selection cursor (`arrangement_view`'s own — not a
    /// REAPER-style DAW-level selection). Draws an accent outline.
    #[props(default)]
    selected: bool,
) -> Element {
    // The store first, the prop as the seed — see `use_live_track`.
    let row_h = height;
    let live = use_live_track(&track);
    let live = live.read();
    let track = live.as_ref().unwrap_or(&track);
    // Per REAPER level. Modest — the name field is the thing that
    // shrinks to make room, and it isn't wide to begin with.
    const INDENT_PX: f32 = 10.0;
    let indent = depth as f32 * INDENT_PX;

    let theme = daw_theme::Theme::default();
    let tint = track
        .color
        .map(|c| {
            daw_theme_art::dress::panel_tint(daw_theme::Color::rgb(
                (c >> 16) as u8,
                (c >> 8) as u8,
                c as u8,
            ))
        })
        .unwrap_or(theme.chrome.surface_raised);
    // The fields are the track colour, darkened — not a flat grey. Measured
    // against the tint in REAPER's own render: the input combo is 0.75 of
    // it (#752D3F against #9D3C55) and the FX slot 0.64 (#652637). The
    // effect is of a translucent panel over the colour, which is what makes
    // a REAPER row read as one surface rather than grey boxes on a tint.
    let combo = tint.shade(-0.25).css();
    let slot = tint.shade(-0.36).css();
    // The name field is the exception: REAPER paints it the panel's own
    // #262626, flat, whatever the track's colour.
    let field = BODY_GREY;
    let gutter = theme.chrome.hardware.shade(-0.40).css();
    let rule = theme.chrome.hardware.shade(-0.72).css();
    let index_ink = theme.chrome.hardware_mark.shade(0.1).css();
    let name_ink = theme.chrome.hardware_mark.shade(0.62).css();
    let combo_ink = theme.chrome.hardware_mark.shade(0.5).css();
    let caret = theme.chrome.hardware_mark.shade(0.2).css();
    let fx_ink = theme.chrome.hardware_mark.shade(-0.1).css();

    // Where the lanes button lands: above phase in the corner when the row
    // has room for both, tucked under solo when it only has room for one.
    let lanes_top = if row_h >= PHASE_HIDE_H {
        row_h - LANES_FROM_FLOOR
    } else {
        SOLO_TOP + 20.0 + 2.0
    };

    let selection_ring = if selected {
        format!("box-shadow: inset 0 0 0 2px {};", theme.chrome.accent.css())
    } else {
        String::new()
    };

    rsx! {
        div {
            class: "relative shrink-0",
            style: "position:relative; width:{ROW_W}px; height:{row_h + 1.0}px; \
                    border-bottom:1px solid {rule}; {selection_ring}",

            // The track colour is the row: REAPER tints the whole panel,
            // not a stripe on it. It stops at the gutter.
            div {
                style: "position:absolute; left:0; top:0; \
                        width:{TINT_W}px; height:{row_h}px; background:{tint.css()};",
            }
            div {
                style: "position:absolute; left:{TINT_W}px; top:0; \
                        width:{GUTTER_W}px; height:{row_h}px; background:{gutter};",
            }

            // ── The left column ──
            //
            // Measured: a #323232 edge at 0..2, then the tint, then a
            // one-pixel rule at 20 dividing the column from the row. The
            // rule and both glyphs are the tint at 0.75 — the same
            // darkening the input combo gets — so they read as pressed
            // into the panel rather than printed on it.
            div {
                style: "position:absolute; left:0; top:0; width:{3.0 + indent}px; height:{row_h}px; \
                        background:#323232;",
            }
            div {
                style: "position:absolute; left:{COLUMN_RULE_X + indent}px; top:0; \
                        width:1px; height:{row_h}px; background:{combo};",
            }
            // At (7,5): the pushpin's 11x11 box carries the needle's
            // corner, so it sits a pixel up and left of where the old
            // 9x9 star did to keep the head on the same spot.
            div { style: "position:absolute; left:{7.0 + indent}px; top:5px;",
                art::TrackPin { colour: combo.clone() }
            }
            // Only an actual folder-start track gets the folder glyph —
            // it used to draw on every row regardless of `is_folder`.
            if track.is_folder {
                div { style: "position:absolute; left:{7.0 + indent}px; top:{row_h - 10.0}px;",
                    art::TrackFolder { colour: combo.clone() }
                }
            }

            // The row's right edge, which REAPER closes with a dark column
            // at 341 — the boundary between the panel and the arrange
            // view, and the last thing missing from the row's silhouette.
            div {
                style: "position:absolute; left:{ROW_W - 2.0}px; top:0; \
                        width:1px; height:{row_h}px; background:{rule};",
            }

            // The track number, between them.
            div {
                class: "absolute font-mono",
                style: "position:absolute; left:{9.0 + indent}px; top:{row_h / 2.0 - 8.0}px; \
                        font-size:11px; line-height:16px; color:{index_ink};",
                "{index + 1}"
            }

            // ── Row one ──
            //
            // No separate monitor indicator: REAPER shows monitoring *on*
            // the record arm in the track panel — `track_recarm_*` has the
            // variants — rather than as a control of its own, and drawing
            // both put a glyph in the row REAPER does not have.
            //
            // The name field is one long box and the record arm and the
            // volume knob sit *on* it, at its two ends — REAPER's field
            // runs from behind the arm at 26 to behind the knob at 230,
            // and the name is set between them. Drawn as three boxes in a
            // line instead, the arm and the knob had grey gutters either
            // side of them that REAPER does not have.
            div {
                style: "position:absolute; left:{NAME_FIELD_X + indent}px; top:{ROW_ONE}px; \
                        width:{(NAME_FIELD_W - indent).max(0.0)}px; height:{NAME_FIELD_H}px; \
                        background:{field}; \
                        border-radius:{NAME_FIELD_H / 2.0}px 0 0 {NAME_FIELD_H / 2.0}px;",
            }
            div { style: "position:absolute; left:{NAME_FIELD_X + indent + 2.0}px; top:7px;",
                RecordArmButton { track: track.guid.clone(), panel: Panel::Track }
            }
            div {
                style: "position:absolute; left:{58.0 + indent}px; top:{ROW_ONE}px; \
                        width:{(NAME_FIELD_X + NAME_FIELD_W - 58.0 - indent).max(0.0)}px; \
                        height:{NAME_FIELD_H}px; \
                        line-height:{NAME_FIELD_H}px; font-size:11.5px; \
                        color:{name_ink}; white-space:nowrap; overflow:hidden; \
                        text-overflow:ellipsis; \
                        font-family:Fira Sans, DejaVu Sans, sans-serif;",
                "{track.name}"
            }
            // The volume knob, on the field's right end.
            div { style: "position:absolute; left:{VOLUME_KNOB_X}px; top:{ROW_ONE}px;",
                VolumeKnob { track: track.guid.clone() }
            }
            // Pan sits *outside* the field, not on it — at 186 it landed
            // under the volume knob, because that was the sheet's number for
            // a row whose field stopped at 180.
            div { style: "position:absolute; left:{PAN_KNOB_X}px; top:5px;",
                PanKnob { track: track.guid.clone() }
            }
            // The routing widget, and then the FX pill — both measured,
            // both drawn by the components the mixer already uses. The
            // routing button's lanes lie in a row here rather than stacked,
            // which is the only difference between REAPER's two images, and
            // the pill is the track panel's 36-wide family rather than the
            // mixer's 46.
            div { style: "position:absolute; left:{ROUTING_X}px; top:6px;",
                IoButton { track: track.guid.clone(), panel: Panel::Track }
            }
            div { style: "position:absolute; left:{FX_IN_X}px; top:6px;",
                FxButton { track: track.guid.clone(), panel: Panel::Track }
            }

            // ── Row two: envelope, the FX slot, the input combo ──
            // 29, measured: the envelope's block runs 29..48 of the row.
            div { style: "position:absolute; left:29px; top:{ROW_TWO}px;",
                EnvelopeButton { width: 22, height: 20, panel: Panel::Track }
            }
            div {
                style: "position:absolute; left:56px; top:{ROW_TWO}px; \
                        width:34px; height:{FIELD_H}px; background:{slot}; \
                        line-height:{FIELD_H}px; text-align:center; font-size:10px; \
                        color:{fx_ink}; font-family:Fira Sans, DejaVu Sans, sans-serif;",
                "FX"
            }
            div {
                // 91 and 195: REAPER's slot and combo touch — scanned
                // across the band there is no tint between them, where we
                // left a four-column gap — and the combo runs to 285.
                style: "position:absolute; left:91px; top:{ROW_TWO}px; \
                        width:195px; height:{FIELD_H}px; background:{combo};",
                div {
                    // Centred, as REAPER sets it — left-aligned it read as
                    // a text field rather than the combo it is.
                    style: "line-height:{FIELD_H}px; font-size:11px; text-align:center; \
                            color:{combo_ink}; white-space:nowrap; overflow:hidden; \
                            font-family:Fira Sans, DejaVu Sans, sans-serif;",
                    "{record_input_name(track)}"
                }
                // The combo's caret — the shared triangle, not a glyph.
                Caret { x: 181.0, y: 8.0, ink: caret }
            }

            // ── The meter section: the meter, then mute over solo ──
            //
            // `meterRight` in the theme moves the whole section to the end
            // of the row, and within it the meter comes *first* — a
            // vertical strip against the tint, with mute and solo to its
            // right. The panel sheet draws the meter inside the tint
            // instead, which put it in the middle of the row.
            div {
                style: "position:absolute; left:{TINT_W + GUTTER_BUTTON_X}px; top:3px;",
                MuteButton { track: track.guid.clone(), panel: Panel::Track }
            }
            div {
                style: "position:absolute; left:{TINT_W + GUTTER_BUTTON_X}px; top:{SOLO_TOP}px;",
                SoloButton { track: track.guid.clone(), panel: Panel::Track }
            }
            // Phase in the corner, the lanes button above it — or, when
            // the row is too short for phase but not for lanes, the lanes
            // button tucked under solo instead.
            if row_h >= PHASE_HIDE_H {
                div {
                    style: "position:absolute; \
                            left:{TINT_W + GUTTER_BUTTON_X + 3.0}px; \
                            top:{row_h - PHASE_FROM_FLOOR}px;",
                    PhaseButton { track: track.guid.clone() }
                }
            }
            if row_h >= LANES_HIDE_H {
                div {
                    style: "position:absolute; \
                            left:{TINT_W + GUTTER_BUTTON_X + 1.0}px; top:{lanes_top}px;",
                    FixedLanes { on: false }
                }
            }

            // No scale: there is no column for numbers out here, and REAPER
            // prints them only in the mixer.
            div {
                style: "position:absolute; left:{TINT_W + TCP_METER_X}px; top:2px;",
                TrackMeter {
                    track: track.guid.clone(),
                    width: TCP_METER_W,
                    height: (row_h - 4.0) as u32,
                    scale: false,
                    // The section's own colour: REAPER's meter here shows
                    // nothing at rest, and a trough put two black bars in a
                    // strip that is meant to read as empty.
                    well: gutter.clone(),
                }
            }
        }
    }
}

/// The envelope control panel row — the TCP-side half of an envelope
/// lane.
///
/// Layout is WALTER's `calcEnvFlow`, read out of `rtconfig.txt` rather
/// than invented: elements flow left to right at `element_h` 20 —
/// **arm (20) → bypass (15) → label (min 30) → fader → hide (36)** — and
/// an FX-parameter envelope (`envcp_type==4`) adds the **parammod and
/// learn plates (30 each)**. The value readout takes the right 60. The
/// panel indents 24 (`main_sec + [24 0 -30]`), the row floor is
/// `envcp_min_height 27`, and every part is the envelope panel's own
/// vector art from #117.
#[component]
pub fn EnvcpRow(
    name: String,
    /// The lane's height — the same number the arrange lane draws at,
    /// through `plan_rows`.
    height: f32,
    #[props(default)] armed: bool,
    #[props(default)] bypassed: bool,
    /// An FX-parameter envelope grows the mod/learn plates.
    #[props(default)]
    fx_param: bool,
    /// Fader position 0..1.
    #[props(default = 0.7)]
    value: f32,
    /// The readout's text.
    #[props(default)]
    readout: String,
) -> Element {
    let t = daw_theme::Theme::default();
    // The envelope panel's own dark ground — a step below the track rows
    // it hangs from.
    let bg = t.chrome.hardware_edge.shade(0.05).css();
    let label_ink = t.chrome.hardware_mark.shade(0.35).css();
    let value_ink = t.chrome.accent.css();
    let rule = t.chrome.hardware_edge.shade(-0.3).css();
    // `element_h` at scale 1; the flow compresses below squash height.
    let el_h = 20.0f32;
    let pad_y = ((height - el_h) / 2.0).max(1.0);
    let fader_w = 56.0f32;
    let cap_at = 4.0 + (fader_w - 22.0) * value.clamp(0.0, 1.0);

    rsx! {
        div {
            style: "position:relative; width:{ROW_W}px; height:{height + 1.0}px; \
                    background:{bg}; border-bottom:1px solid {rule}; \
                    overflow:hidden;",
            div {
                style: "position:absolute; left:24px; top:{pad_y}px; \
                        display:flex; align-items:center; height:{el_h}px; \
                        width:{ROW_W - 24.0 - 6.0}px;",
                div { style: "flex:0 0 auto; line-height:0;",
                    art::EnvcpArmButton { armed, cell: (20.0, 20.0) }
                }
                div { style: "flex:0 0 auto; line-height:0;",
                    art::EnvcpBypassButton { bypassed, cell: (15.0, 20.0) }
                }
                div {
                    style: "flex:1 1 auto; min-width:30px; padding:0 4px; \
                            font-size:9px; color:{label_ink}; white-space:nowrap; \
                            overflow:hidden; text-overflow:ellipsis; \
                            font-family:Fira Sans, DejaVu Sans, sans-serif;",
                    "{name}"
                }
                // The horizontal fader: the slab, and the cap slid to the
                // value.
                div {
                    style: "flex:0 0 auto; position:relative; width:{fader_w}px; \
                            height:{el_h}px;",
                    div { style: "position:absolute; left:0; top:6px; line-height:0;",
                        art::EnvcpPanel {
                            part: art::EnvcpPart::FaderTrack,
                            cell: (fader_w, 8.0),
                        }
                    }
                    div { style: "position:absolute; left:{cap_at}px; top:0; line-height:0;",
                        art::EnvcpPanel {
                            part: art::EnvcpPart::FaderCap,
                            cell: (22.0, 20.0),
                        }
                    }
                }
                if fx_param {
                    div { style: "flex:0 0 auto; line-height:0; margin-left:4px;",
                        art::EnvcpPlate { glyph: art::EnvcpGlyph::ParamMod }
                    }
                    div { style: "flex:0 0 auto; line-height:0;",
                        art::EnvcpPlate { glyph: art::EnvcpGlyph::Learn }
                    }
                }
                div { style: "flex:0 0 auto; line-height:0; margin-left:4px;",
                    art::EnvcpOptionsButton { cell: (36.0, 20.0) }
                }
                if !readout.is_empty() {
                    div {
                        style: "flex:0 0 auto; width:52px; text-align:right; \
                                font-size:8px; color:{value_ink}; \
                                font-variant-numeric:tabular-nums; \
                                font-family:DejaVu Sans Mono, monospace;",
                        "{readout}"
                    }
                }
            }
        }
    }
}

/// The volume knob, on the name field's right end.
///
/// The art is `vector_controls::VolumeKnob`; this is the wrapper that binds
/// it to the track. The ring swings on the *taper*, not the gain, for the
/// same reason the fader's cap does — unity is a little under
/// three-quarters round, not hard against the stop.
#[component]
fn VolumeKnob(track: String) -> Element {
    let mut at = use_signal(art::Interaction::default);
    let store = use_track_store();
    let guid = track.clone();
    let gain = use_memo(use_reactive!(|guid| store.volume(&guid)));

    rsx! {
        div {
            style: "display:inline-block; line-height:0; cursor:ns-resize;",
            onmouseenter: move |_| at.set(art::Interaction::Hover),
            onmouseleave: move |_| at.set(art::Interaction::Normal),
            art::VolumeKnob {
                value: crate::controls::fader_position(gain()) as f32,
                width: Some(24),
                height: Some(24),
                at: at(),
            }
        }
    }
}

/// The fixed-lanes button, bound to nothing yet.
///
/// `trackfixedlanes` is not something the DAW facade reports, so this
/// draws the off state rather than inventing one — the same stance the
/// envelope button takes.
#[component]
fn FixedLanes(on: bool) -> Element {
    let mut at = use_signal(art::Interaction::default);
    rsx! {
        div {
            style: "display:inline-block; line-height:0; cursor:pointer;",
            onmouseenter: move |_| at.set(art::Interaction::Hover),
            onmouseleave: move |_| at.set(art::Interaction::Normal),
            art::FixedLanesButton { on, width: Some(20), height: Some(24), at: at() }
        }
    }
}
