//! Mixer Panel — DAW mixer with channel strips resembling REAPER's mixer.
//!
//! Each channel strip shows (top to bottom):
//! - The FX pill
//! - Mute / Solo / FX buttons
//! - Volume fader
//! - dB readout + pan label
//! - Record arm / monitoring buttons
//! - Track name + number

use crate::controls::{
    Caret, Collapse, ControlSync, EnvelopeButton, FxButton, IoButton, MeterFeed, MonitorButton,
    MuteButton, PanAnchor, PanKnob, PhaseButton, RecordArmButton, RecordInputLabel, SoloButton,
    TrackMeter, TrackName, VolumeFader, VolumeWidget, record_input_name, use_daw_tracks,
    use_live_track, use_track_store,
};
use crate::prelude::*;
use daw_proto::Track;

/// Mixer panel that polls the DAW for track state.
#[component]
pub fn MixerPanel(
    /// The height the strips are drawn at.
    ///
    /// One number, not two: the strip is drawn at this height *and*
    /// collapses by it. `h-full` plus a separate collapse prop let the two
    /// disagree silently — the strip resolved its bands for one height and
    /// was stretched to another, so every measured offset landed in the
    /// wrong band.
    ///
    /// The default is a *seed*, not the answer: the panel measures its own
    /// strip row on mount and draws at what it finds, so a short dock does
    /// not clip the name plate and a tall one actually reaches the
    /// [`Collapse`] thresholds. The prop remains for a caller that knows
    /// its box better than a measurement would.
    #[props(default = 371.0)]
    height: f32,
) -> Element {
    let mut tracks = use_signal(Vec::<Track>::new);
    let mut error_msg = use_signal(|| Option::<String>::None);
    let mut connected = use_signal(|| false);
    let mut measured_h = use_signal(|| Option::<f32>::None);

    // The vector controls read their track from here, and it is fed by the
    // track-event subscription rather than by this panel's two-second poll:
    // a mute performed in REAPER shows up as soon as the event lands.
    use_daw_tracks(use_track_store());

    // Poll for tracks
    use_future(move || async move {
        loop {
            if daw_control::Daw::try_get().is_some() {
                break;
            }
            // futures-timer, not tokio: this panel runs inside REAPER on
            // dioxus's own scheduler, where there is no tokio runtime and a
            // tokio timer is a non-unwinding panic that takes the host down
            // with it.
            futures_timer::Delay::new(std::time::Duration::from_millis(500)).await;
        }

        let daw = daw_control::Daw::get();
        connected.set(true);

        loop {
            match daw.current_project().await {
                Ok(project) => match project.tracks().all().await {
                    Ok(track_list) => {
                        tracks.set(track_list);
                        error_msg.set(None);
                    }
                    Err(e) => {
                        error_msg.set(Some(format!("Failed to fetch tracks: {:?}", e)));
                    }
                },
                Err(e) => {
                    error_msg.set(Some(format!("Failed to get project: {:?}", e)));
                }
            }

            futures_timer::Delay::new(std::time::Duration::from_secs(2)).await;
        }
    });

    let is_connected = *connected.read();

    if !is_connected {
        return rsx! {
            div { class: "h-full w-full flex items-center justify-center bg-card text-muted-foreground text-sm",
                "Waiting for DAW connection..."
            }
        };
    }

    {
        let err = error_msg.read();
        if let Some(msg) = err.as_ref() {
            let msg = msg.clone();
            return rsx! {
                div { class: "h-full w-full flex items-center justify-center bg-card text-red-400 text-sm p-4",
                    "{msg}"
                }
            };
        }
    }

    let track_list = tracks.read().clone();
    // Measured beats seeded: the strips draw at the row the dock actually
    // gave them, or the prop until the measurement lands.
    let strip_h = measured_h.read().unwrap_or(height);

    rsx! {
        div { class: "h-full w-full flex flex-col bg-zinc-900 overflow-hidden",
            // The single place a drag becomes an engine write.
            ControlSync {}
            // One meter subscription for the whole mixer. Mounted here
            // rather than per strip: the frame carries every track, and the
            // engine's pump is gated on there being a subscriber at all.
            MeterFeed {}

            // Header
            div { class: "px-3 py-1.5 border-b border-zinc-700 flex items-center justify-between flex-shrink-0",
                h2 { class: "text-xs font-semibold text-zinc-300", "Mixer" }
                span { class: "text-[10px] text-zinc-500", "{track_list.len()} tracks" }
            }

            // Channel strips — horizontal scroll
            div {
                class: "flex-1 overflow-x-auto overflow-y-hidden",
                // The same measurement arrange_view takes: the strips are
                // drawn — and collapsed — at the row's real height, not at
                // a number chosen before the dock existed.
                // `onmounted` gives the initial height; `onresize` (a real
                // ResizeObserver) keeps it current as the window/dock
                // resizes afterwards — matching the arrangement view's fix
                // for the same staleness.
                onresize: move |evt| {
                    if let Ok(size_box) = evt.get_content_box_size() {
                        if size_box.height > 0.0 {
                            measured_h.set(Some(size_box.height as f32));
                        }
                    }
                },
                onmounted: move |evt| {
                    spawn(async move {
                        if let Ok(rect) = evt.get_client_rect().await {
                            if rect.size.height > 0.0 {
                                measured_h.set(Some(rect.size.height as f32));
                            }
                        }
                    });
                },
                div { class: "flex h-full",
                    for (i, track) in track_list.iter().enumerate() {
                        ChannelStrip {
                            key: "{track.guid}",
                            track: track.clone(),
                            index: i as u32,
                            height: strip_h,
                        }
                    }
                }
            }
        }
    }
}

/// One strip, for a caller that already has its track.
///
/// [`MixerPanel`] polls the DAW for a track list and owns the subscriptions;
/// a test, a playground or a design review has the tracks in hand and wants
/// only the strip. Public so the assembled strip can be exercised — and
/// photographed — without a backend behind it.
#[component]
pub fn ChannelStripPreview(
    track: Track,
    #[props(default)] index: u32,
    /// The strip's height in px. Drives the collapse — see
    /// [`Collapse`][crate::controls::Collapse].
    #[props(default = 371.0)]
    height: f32,
) -> Element {
    rsx! {
        ChannelStrip { track, index, height }
    }
}

// The strip's geometry — every rtconfig-derived and measured number — lives
// in `daw_theme_art::geometry::mcp`, one home for the panel's facts, guarded
// against the theme by `fts-themer thresholds --check`. The section heights
// come from the same constants `Collapse` computes the stretch residual
// with, so a remeasurement there cannot leave the drawn sections and the
// residual arithmetic disagreeing.
use daw_theme_art::collapse::{BOTTOM_SECTION, FX_SECTION};
use daw_theme_art::geometry::mcp::{
    ARM_CELL_H, ARM_LEFT, ARM_OVERHANG, BUTTON_W, COLUMN, ENV_FROM_FLOOR, FX_PILL_TOP, IN_FIELD_W,
    INPUT_FIELD_H, IO_FROM_SOLO, METER_W, MUTE_FROM_RECMON, NAME_PLATE, PAN_KNOB_W, PHASE_FROM_ENV,
    PHASE_W, RECMON_FROM_ARM, SOLO_FROM_MUTE, STRIP_W,
};

/// One control in the right-hand column.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Stacked {
    Monitor,
    Mute,
    Solo,
    Io,
    Phase,
    Envelope,
}

impl Stacked {
    /// Where the control's own box starts, given the column's left edge.
    ///
    /// Everything sits on the column except the IO button, which
    /// `rtconfig` writes as `mcp.solo + [-1 23 23 30]` — one pixel left and
    /// two wider than the buttons above it, on purpose.
    fn left(self, column: f32) -> f32 {
        match self {
            Self::Io => column - 1.0,
            // 16 wide against the column's 21, so it centres rather than
            // hanging off the left.
            Self::Phase => column + (BUTTON_W - PHASE_W) / 2.0,
            // `mcp.env` is written `+ [1 ...]` off the IO button, which is
            // itself a column left — so it lands back on the column.
            Self::Envelope => column,
            _ => column,
        }
    }
}
/// The strip's own grey — `mcp_namebg`'s, and the same one the plate under
/// the track name uses. A measured token, not a local hex, so a re-palette
/// reaches it (#240).
use daw_theme::defaults::STRIP_BODY as BODY_GREY;

// ── Channel Strip ───────────────────────────────────────────────────

#[derive(Props, Clone)]
struct ChannelStripProps {
    track: Track,
    index: u32,
    /// The height the strip is drawn at, which is what it collapses by.
    ///
    /// REAPER's own default MCP height; a panel that knows its box passes
    /// the real one.
    #[props(default = 371.0)]
    height: f32,
}

impl PartialEq for ChannelStripProps {
    fn eq(&self, other: &Self) -> bool {
        self.track == other.track && self.index == other.index && self.height == other.height
    }
}

#[component]
fn ChannelStrip(props: ChannelStripProps) -> Element {
    // The store first, the prop as the seed — see `use_live_track`.
    let live = use_live_track(&props.track);
    let live = live.read();
    let track = live.as_ref().unwrap_or(&props.track);

    let vol_db = if track.volume > 0.0 {
        20.0 * track.volume.log10()
    } else {
        -100.0
    };

    let db_label = if vol_db > -100.0 {
        format!("{:.1}", vol_db)
    } else {
        "-inf".to_string()
    };

    // Resolved once, so the markup asks a question rather than doing the
    // arithmetic in six places.
    let shape = Collapse::at(props.height);

    // The tinted band is `pan_sec` extended by `in_sec`, in the track's own
    // colour. An uncoloured track takes the panel grey rather than a colour
    // invented for it.
    let theme = daw_theme::Theme::default();
    let tint_colour = track
        .color
        .map(|c| {
            daw_theme_art::dress::panel_tint(daw_theme::Color::rgb(
                (c >> 16) as u8,
                (c >> 8) as u8,
                c as u8,
            ))
        })
        .unwrap_or(theme.chrome.surface_raised);
    let tint = tint_colour.css();
    // The fields the input section holds, darkened out of the tint the way
    // the track panel's are — measured there at 0.75 of it.
    let field = tint_colour.shade(-0.25).css();
    let ink = theme.chrome.hardware_mark.shade(0.5).css();
    let faint = theme.chrome.hardware_mark.shade(0.1).css();
    let caret = theme.chrome.hardware_mark.shade(0.2).css();
    // One row of highlight along the band's top edge — `mcp.custom.bg_hl_t`.
    // Without it the band starts flat where the source starts lit.
    let tint_hl = tint_colour.shade(0.13).css();

    // The band's height comes from the same place the stretch residual
    // does. It used to be worked out twice — 33-or-6 here against
    // `Collapse`'s 50/33/6 there — so the sections did not tile the strip
    // and the meter was seventeen rows shorter than the band it sat in.
    // Exactly the bug the strip's own height had, one level down.
    let (pan_h, input_h) = (shape.pan_band, shape.input_band);
    let band = pan_h + input_h;

    // The button column, resolved on REAPER's chain. `mcp.recmon` hangs off
    // the record arm, which hangs through the band's floor — so the column's
    // first row is fixed relative to the arm rather than to the section.
    let column = {
        let mut at = ARM_OVERHANG - ARM_CELL_H + RECMON_FROM_ARM;
        let mut out = vec![(at, Stacked::Monitor)];
        for (step, control, shown) in [
            (MUTE_FROM_RECMON, Stacked::Mute, true),
            (SOLO_FROM_MUTE, Stacked::Solo, true),
            (IO_FROM_SOLO, Stacked::Io, shape.show_io),
        ] {
            at += shape.padding + step;
            if shown {
                out.push((at, control));
            }
        }
        // Envelope and phase hang off the *bottom* of the stretch section,
        // not off the chain above them — which is why REAPER's column
        // spreads as a strip grows instead of staying a cluster at the top:
        //
        //     mcp.env   = mcp.io + [0 stretch_sec{3}] + [1 -30 21 30]
        //     mcp.phase = mcp.env - [0 padding] + [3 -18 16 18]
        //
        // `stretch_sec{3}` is the section's height, so env sits 30 above
        // its floor and phase 18 above env.
        let floor = shape.stretch - ENV_FROM_FLOOR;
        if shape.show_phase {
            out.push((floor - shape.padding - PHASE_FROM_ENV, Stacked::Phase));
        }
        if shape.show_envelope {
            out.push((floor, Stacked::Envelope));
        }
        out
    };
    // `mcp.meter` is inset 4 on every side of the stretch section.
    let meter_h = (shape.stretch - 8.0).max(24.0) as u32;

    let selected_border = if track.selected {
        "border-blue-500"
    } else {
        "border-zinc-700"
    };

    rsx! {
        div {
            class: "flex flex-col shrink-0 border-r {selected_border}",
            // The section stack is stated inline as well as in Tailwind.
            // Blitz windows embed the sheet as a static string and a panel
            // that mounts before it — or a test that mounts without it —
            // gets no `flex` at all: the fixed bands stack at their content
            // height and the stretch band collapses to nothing, so the
            // bottom section paints over the meter. Tailwind is the
            // additive layer here, not the structure.
            // #262626 stated inline, not `bg-zinc-900`: REAPER's strip body
            // is the same grey as `mcp_namebg`, which is why the plate under
            // the name looked right while everything above it sat in a
            // darker box. Inline because a class-only background disappears
            // in any window that renders before the sheet.
            style: "width:{STRIP_W}px; height:{props.height}px; \
                    display:flex; flex-direction:column; background:{BODY_GREY};",

            // Laid out on REAPER's own geometry, in Tailwind and explicit
            // pixels rather than by eye.
            //
            // Every number below is measured — the section heights and the
            // control offsets come from `rtconfig.txt` by way of
            // `daw-theme-art`'s panel sheet, which composes these same
            // components at those coordinates. The first version of this
            // strip stacked them in a flex column with its own paddings and
            // no dB scale, and the two mixers did not read as the same
            // mixer at a glance, which is the thing #135 is for.
            //
            // Structure is Tailwind (flex, flex-1, relative); position is
            // px, because REAPER's layout is a coordinate system and
            // pretending otherwise loses the match.

            // ── FX section (33 rows) ────────────────────────────
            //
            // `mcp.fx` is [7 7 43 20] of the section, with `mcp.fxbyp`
            // butted onto its right — the pill is one shape in two images.
            div {
                class: "relative shrink-0",
                style: "flex:0 0 auto; position:relative; height:{FX_SECTION}px;",
                // 9, not `mcp.fx`'s nominal 7. Measured against REAPER with
                // the coloured bands aligned: its pill's face runs 11..26 of
                // the section and ours ran 9..24 — the same 16 rows, two
                // high. The section carries a top inset the box coordinates
                // do not mention.
                div { style: "position:absolute; left:7px; top:{FX_PILL_TOP}px;",
                    FxButton { track: track.guid.clone(), width: 43.0 }
                }
            }

            // ── The tinted band: pan + input ────────────────────
            //
            // `mcp.custom.bg` is `pan_sec` extended by `in_sec`'s height,
            // in the track's own colour with one lit row along its top.
            // Fixed both ways — `grow-0 shrink-0` and a stated height — so
            // the band is the same depth on every strip whatever is inside
            // it. Left to the flex default a strip whose pan label had
            // collapsed sat a few rows shorter than its neighbours.
            div {
                class: "relative shrink-0 grow-0",
                style: "flex:0 0 auto; position:relative; height:{band}px; \
                        min-height:{band}px; max-height:{band}px; \
                        background:{tint}; border-top:1px solid {tint_hl};",

                // Pan at local x 9.5, the record arm on the right-hand
                // column's axis — both placed on the centres the source
                // gives, not on a corner.
                // The pan control is here either way — what changes is the
                // section around it. Compressed, `mcp.pan` re-anchors off
                // `mcp.recmode` and both it and the record arm sit inside
                // the band, pan at its left and the arm at its right, which
                // is where the source has them.
                // Centred on the strip, and at the top of the section: the
                // knob is 25 rows and the label 7, which is exactly the 33
                // the section has — two rows of slack and the label runs
                // under the record-input field below it.
                div { style: "position:absolute; left:{(STRIP_W - PAN_KNOB_W) / 2.0}px; top:0;",
                    PanKnob { track: track.guid.clone() }
                    if shape.show_pan_labels {
                        // Inline, like everything else that decides layout
                        // here: a Tailwind-only `text-[8px]` falls back to
                        // the UA's 16px in any window that renders before
                        // the sheet, and a 16px "pan" is half the band.
                        // Inside the pan section, not over the input field
                        // below it: the knob is 25 rows of a 33-row section
                        // and a label on its own line ran past the edge.
                        div {
                            class: "text-[8px] text-zinc-400 leading-none text-center",
                            style: "position:absolute; left:0; top:26px; width:{PAN_KNOB_W}px; \
                                    font-size:7px; line-height:7px; text-align:center; \
                                    color:#a1a1aa; \
                                    font-family:Fira Sans, DejaVu Sans, sans-serif;",
                            "pan"
                        }
                    }
                }
                // Hung *through* the band's floor, not sat on it.
                //
                // In REAPER the housing spans rows 35..56 of a strip whose
                // coloured band ends at 52 — the last four rows land on the
                // dark below, so the round housing reads as one shape with
                // the background it is anchored to. Kept inside the band the
                // button floated in the colour with a seam under it.
                //
                // `z-index` because the stretch section is the next sibling
                // and would otherwise paint over the overhang.
                div {
                    style: "position:absolute; left:{ARM_LEFT}px; \
                            bottom:-{ARM_OVERHANG}px; z-index:2;",
                    RecordArmButton { track: track.guid.clone() }
                }
                // The input section — `mcp.recinput` over `mcp.recmode`,
                // with the input-FX mark between them:
                //
                //     mcp.recinput = in_sec + [6 0 75 16]
                //     mcp.recmode  = in_sec + [6 4 42 16] + [0 16]
                //
                // Both are dropdowns, so both carry a caret; REAPER prints
                // the input's name in the first and the mode in the second.
                if shape.show_record_input {
                    div {
                        style: "position:absolute; left:6px; top:{pan_h}px; \
                                width:{STRIP_W - 12.0}px; height:{INPUT_FIELD_H}px; \
                                background:{field};",
                        div {
                            style: "padding:0 12px 0 4px; line-height:{INPUT_FIELD_H}px; \
                                    font-size:9px; color:{ink}; white-space:nowrap; \
                                    overflow:hidden; text-overflow:ellipsis; \
                                    font-family:Fira Sans, DejaVu Sans, sans-serif;",
                            "{record_input_name(track)}"
                        }
                        Caret { x: STRIP_W - 12.0 - 10.0, y: INPUT_FIELD_H / 2.0 - 2.0, ink: caret.clone() }
                    }
                    // `IN FX` — the input chain's own mark, which REAPER
                    // sets small and hard against the section's right.
                    div {
                        style: "position:absolute; right:6px; top:{pan_h + INPUT_FIELD_H}px; \
                                font-size:7px; line-height:8px; color:{faint}; \
                                font-family:Fira Sans, DejaVu Sans, sans-serif;",
                        "IN FX"
                    }
                    div {
                        style: "position:absolute; left:6px; \
                                top:{pan_h + INPUT_FIELD_H + 5.0}px; \
                                width:{IN_FIELD_W}px; height:{INPUT_FIELD_H}px; \
                                background:{field};",
                        div {
                            style: "padding:0 0 0 6px; line-height:{INPUT_FIELD_H}px; \
                                    font-size:9px; color:{ink}; \
                                    font-family:Fira Sans, DejaVu Sans, sans-serif;",
                            "IN"
                        }
                        Caret { x: IN_FIELD_W - 12.0, y: INPUT_FIELD_H / 2.0 - 2.0, ink: caret.clone() }
                    }
                }
            }

            // ── Stretch: meter, fader, and the button column ────
            //
            // `mcp.meter` is one rect covering the bars *and* the scale;
            // the fader sits at x 28; and everything in the right-hand
            // column shares one axis, each control anchored to the last
            // exactly as `rtconfig` writes it:
            //
            //     mcp.recmon = mcp.recarm + [7 20 21 20]
            //     mcp.mute   = mcp.recmon + [0 19 21 20]
            //     mcp.solo   = mcp.mute   + [0 21 21 20]
            //     mcp.io     = mcp.solo   + [-1 23 23 30]
            div {
                class: "relative flex-1 min-h-0",
                style: "flex:1 1 0; min-height:0; position:relative;",
                // The numeric readout, which is a different thing from the
                // scale printed inside the meter: `hide_volume_label` drops
                // this one at 250 while the scale stays.
                if shape.show_volume_label {
                    div {
                        class: "absolute font-mono text-[9px] text-zinc-400",
                        style: "position:absolute; left:4px; top:-11px; \
                                font-size:9px; line-height:9px; color:#a1a1aa; \
                                font-family:DejaVu Sans Mono, monospace;",
                        "{db_label}"
                    }
                }
                // Both boxes state a height in px rather than `top/bottom`:
                // the fader's rail is `height:100%`, and a percentage of an
                // absolutely-positioned box with no stated height resolves
                // to nothing — the meter looked right only because its own
                // art carries pixel dimensions.
                div { style: "position:absolute; left:4px; top:4px; height:{meter_h}px;",
                    TrackMeter {
                        track: track.guid.clone(),
                        width: METER_W,
                        height: meter_h,
                        // The strip's own grey, not a dark trough: REAPER's
                        // meter is the panel until something lights it, and
                        // a well made the block read as a cut-out sitting
                        // on the strip rather than part of it.
                        well: BODY_GREY.to_string(),
                    }
                }
                div { style: "position:absolute; left:28px; top:4px; height:{meter_h}px;",
                    match shape.volume {
                        // Below the swap threshold a fader has no travel
                        // worth having, so it stops being a fader.
                        VolumeWidget::Fader => rsx! {
                            VolumeFader { track: track.guid.clone(), height: meter_h as f32 }
                        },
                        VolumeWidget::Knob => rsx! {
                            PanKnob { track: track.guid.clone(), large: true }
                        },
                    }
                }
                // REAPER's own offset chain, not a uniform flex gap.
                //
                //     mcp.recmon = mcp.recarm + [7 20 21 20]
                //     mcp.mute   = mcp.recmon + [0 padding] + [0 19 21 20]
                //     mcp.solo   = mcp.mute   + [0 padding] + [0 21 21 20]
                //     mcp.io     = mcp.solo   + [0 padding] + [-1 23 23 30]
                //
                // The steps are not equal — 19 against a 20-tall button is a
                // one-row *overlap* before padding, and the IO button is
                // deliberately a column left and two wider. A single `gap`
                // got the first button right and then drifted, five rows by
                // the time it reached IO.
                for (top, control) in column {
                    div {
                        key: "{control:?}",
                        style: "position:absolute; left:{control.left(COLUMN)}px; \
                                top:{top}px;",
                        match control {
                            Stacked::Monitor => rsx! { MonitorButton { track: track.guid.clone() } },
                            Stacked::Mute => rsx! { MuteButton { track: track.guid.clone() } },
                            Stacked::Solo => rsx! { SoloButton { track: track.guid.clone() } },
                            Stacked::Io => rsx! { IoButton { track: track.guid.clone() } },
                            Stacked::Phase => rsx! { PhaseButton { track: track.guid.clone() } },
                            Stacked::Envelope => rsx! { EnvelopeButton {} },
                        }
                    }
                }
            }

            // ── Bottom (47 rows: 26 of name, 20 of index) ──────
            div {
                class: "shrink-0 grow-0",
                style: "flex:0 0 auto; height:{BOTTOM_SECTION}px; \
                        min-height:{BOTTOM_SECTION}px; max-height:{BOTTOM_SECTION}px;",
                // 26 of the section's 47, with the index plate's 20 and a
                // dividing row under it. Left to its content the plate was
                // 16 tall and the strip ended in a band of dead grey.
                TrackName { track: track.guid.clone(), width: STRIP_W as u32, height: NAME_PLATE }
                div {
                    class: "text-center",
                    // `text-align` inline as well: without the sheet the
                    // index sat against the left edge of its own plate.
                    style: "height:20px; line-height:20px; font-size:11px; \
                            text-align:center; color:#f0f0f0; background:{tint}; \
                            border-top:1px solid {tint_hl};",
                    "{track.index + 1}"
                }
            }
        }
    }
}
