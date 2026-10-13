//! A whole track panel and mixer strip, drawn from the components.
//!
//!     cargo run -p daw-theme-art --example panel
//!     → target/theme-shots/native-panels.png
//!
//! Everything up to now has been one control against one PNG. This is the
//! question those were for: put the controls together the way REAPER lays
//! them out and see whether the result reads as the same panel.
//!
//! The layout follows a 1x REAPER screenshot of this theme rather than
//! anything invented here — a 296-wide track panel over 71-row tracks, an
//! 86-wide mixer strip — so the two can be set side by side and compared
//! honestly. What is *not* here is as much the point as what is: the
//! meters, the dB scale and the input combo have no component yet and are
//! drawn as plain rectangles, marked in the report.
//!
//! It composes by *nesting* each control's own `<svg>` inside a parent
//! with `x`/`y`, rather than re-rendering into a shared coordinate space.
//! That keeps every control's measured cell exactly as it is audited — a
//! control that scores 0.21 alone scores 0.21 here — so this file holds
//! only layout, no geometry.

use daw_theme::Color;
use daw_theme_art::render::{rasterise, render_svg};
use daw_theme_art::slice::{Slice, expect_art as art};
use daw_theme_art::vector_controls as v;

/// The routing button as *this drawing* wants it: two rows short of
/// `mcp_io`'s 23x32 source, which is what the reference shot measures.
///
/// Not a table entry, and not `..art("mcp_io_s_r")` either. `MCP_ART`
/// records what REAPER ships, measured; a `source` that disagrees with the
/// art while carrying the art's declared `slice` would be neither, and the
/// bands are stated in `source`'s units, so the two cannot be mixed. This
/// says plainly that it is the demo's own box. `FaderCapProps::full` and
/// `FxControlProps::widen` have since gone, replaced by the pane
/// decomposition; this sheet still draws whole art at its own proportions
/// because it is photographing controls, not laying out a strip.
const IO_DRAWN_SHORT: v::NamedArt = v::NamedArt {
    name: "mcp_io_s_r, drawn short",
    source: (23.0, 30.0),
    slice: Slice::FIXED,
};

const NONE: (Option<u32>, Option<u32>) = (None, None);

/// One control, placed.
///
/// `svg` is a whole document from `render_svg`, and it is nested rather
/// than re-rendered: only `x` and `y` are added, so the control keeps the
/// width, height and viewBox it was audited at. Adding a second `width`
/// beside the one already there is a parse error rather than an override,
/// which is how this was written first.
fn at(x: f32, y: f32, svg: &str) -> String {
    // Gradient ids are fixed per component — `lbM`, `trlit`, `recring` —
    // which is fine for one control in one document and wrong the moment
    // two of the same control share a page: the first definition wins and
    // every later copy silently paints with it. Five mute buttons came
    // out in the first one's grey, with a one-row red sliver where the
    // highlight is drawn from a colour rather than a reference.
    //
    // Uniquing them here rather than in the components keeps the exported
    // PNGs byte-identical, but it is a real constraint on the live UI and
    // wants solving there properly.
    static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let svg = svg.replace("id=\"", &format!("id=\"u{n}"));
    let svg = svg.replace("url(#", &format!("url(#u{n}"));
    // Only add `preserveAspectRatio` if the component did not state one.
    // Some now do — the sliced controls have to, since a stretch band that
    // letterboxes is the bug the slice exists to prevent — and a duplicate
    // attribute is not a warning: resvg rejects the whole document, and the
    // sheet silently keeps yesterday's PNG.
    let stretch = if svg.contains("preserveAspectRatio") {
        ""
    } else {
        "preserveAspectRatio=\"none\" "
    };
    svg.strip_prefix("<svg ")
        .map(|s| format!("<svg x=\"{x}\" y=\"{y}\" {stretch}{s}"))
        .unwrap_or_else(|| svg.to_string())
}

fn rect(x: f32, y: f32, w: f32, h: f32, fill: &str) -> String {
    format!("<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" fill=\"{fill}\"/>")
}

fn label(x: f32, y: f32, size: f32, fill: &str, s: &str) -> String {
    format!(
        "<text x=\"{x}\" y=\"{y}\" font-family=\"Fira Sans, DejaVu Sans, sans-serif\" \
         font-size=\"{size}\" fill=\"{fill}\">{s}</text>"
    )
}

struct Track {
    name: &'static str,
    tint: Color,
    armed: bool,
    muted: bool,
    soloed: bool,
}

fn mute(art: v::NamedArt, body: (f32, f32), track: bool, on: bool) -> String {
    let t = daw_theme::Theme::default();
    render_svg(
        v::MuteButton,
        v::ToggleProps {
            on,
            art,
            body,
            unlit: Some(t.chrome.hardware.shade(if track { 0.078 } else { 0.036 })),
            legend: Some(t.chrome.hardware_mark.shade(match (track, on) {
                (true, true) => 0.86,
                (true, false) => 0.23,
                (false, _) => 0.45,
            })),
            sinks: !track,
            hover: 0.25,
            depth: 0.11,
            width: NONE.0,
            height: NONE.1,
            at: v::Interaction::Normal,
        },
    )
}

fn solo(art: v::NamedArt, body: (f32, f32), track: bool, on: bool) -> String {
    let t = daw_theme::Theme::default();
    render_svg(
        v::SoloButton,
        v::SoloProps {
            state: if on { v::Solo::On } else { v::Solo::Off },
            art,
            body,
            unlit: Some(t.chrome.hardware.shade(if track { 0.078 } else { 0.036 })),
            legend: Some(t.chrome.hardware_mark.shade(match (track, on) {
                (true, true) => 1.0,
                (true, false) => 0.44,
                (false, true) => 0.59,
                (false, false) => 0.45,
            })),
            sinks: !track,
            hover: 0.35,
            depth: 0.11,
            width: NONE.0,
            height: NONE.1,
            at: v::Interaction::Normal,
        },
    )
}

/// A meter, placed. `Meter` draws the bars, the well and the scale.
fn meter(x: f32, y: f32, w: f32, h: f32, levels: Vec<f32>, marks: &[&str]) -> String {
    at(
        x,
        y,
        &render_svg(
            v::Meter,
            v::MeterProps {
                well: None,
                holds: Vec::new(),
                levels,
                cell: (w, h),
                scale: !marks.is_empty(),
                marks: marks.iter().map(|m| m.to_string()).collect(),
                width: Some(w as u32),
                height: Some(h as u32),
                at: v::Interaction::Normal,
            },
        ),
    )
}

fn track_row(y: f32, n: u32, tk: &Track) -> String {
    let t = daw_theme::Theme::default();
    let mut s = String::new();

    // The track colour is the row: REAPER tints the whole panel, not a
    // stripe on it.
    s.push_str(&rect(0.0, y, 296.0, 70.0, &tk.tint.css()));
    s.push_str(&rect(
        0.0,
        y + 70.0,
        296.0,
        1.0,
        &t.chrome.hardware.shade(-0.72).css(),
    ));
    s.push_str(&label(
        9.0,
        y + 44.0,
        11.0,
        &t.chrome.hardware_mark.shade(0.1).css(),
        &n.to_string(),
    ));

    // Row one: arm, name, pan, meter, FX, IN.
    s.push_str(&at(
        26.0,
        y + 6.0,
        &render_svg(
            v::RecordArmButton,
            v::RecordArmProps {
                state: if tk.armed {
                    v::RecordArm::On
                } else {
                    v::RecordArm::Off
                },
                art: art("track_recarm_on"),
                housing: false,
                width: NONE.0,
                height: NONE.1,
                at: v::Interaction::Normal,
            },
        ),
    ));
    s.push_str(&rect(
        50.0,
        y + 7.0,
        130.0,
        17.0,
        &t.chrome.hardware.shade(-0.40).css(),
    ));
    s.push_str(&label(
        56.0,
        y + 20.0,
        11.5,
        &t.chrome.hardware_mark.shade(0.62).css(),
        tk.name,
    ));
    s.push_str(&at(
        186.0,
        y + 3.0,
        &render_svg(
            v::PanningKnob,
            // Centred, like every track in the reference shot. A panned
            // knob here is honest demo data and reads as a broken
            // pointer, which is not what this render is for.
            v::PanProps {
                position: 0.0,
                large: false,
                indicator: true,
                width: NONE.0,
                height: NONE.1,
            },
        ),
    ));
    s.push_str(&meter(214.0, y + 8.0, 20.0, 15.0, vec![0.7, 0.5, 0.6], &[]));
    s.push_str(&at(
        242.0,
        y + 6.0,
        &render_svg(
            v::FxInButton,
            v::FxInProps {
                loaded: n.is_multiple_of(2),
                cell: (29.0, 20.0),
                width: NONE.0,
                height: NONE.1,
                at: v::Interaction::Normal,
            },
        ),
    ));
    s.push_str(&at(
        274.0,
        y + 6.0,
        &render_svg(
            v::InputMonitorIndicator,
            v::MonitoringProps {
                state: if tk.armed {
                    v::Monitoring::On
                } else {
                    v::Monitoring::Off
                },
                art: art("track_monitor_on"),
                axis: v::Axis::Horizontal,
                width: NONE.0,
                height: NONE.1,
                at: v::Interaction::Normal,
            },
        ),
    ));

    // Row two: envelope, the FX slot, the input combo.
    s.push_str(&at(
        26.0,
        y + 32.0,
        &render_svg(
            v::EnvelopeButton,
            v::EnvelopeProps {
                scrim: true,
                mode: v::EnvelopeMode::Off,
                cell: (22.0, 20.0),
                width: NONE.0,
                height: NONE.1,
                at: v::Interaction::Normal,
            },
        ),
    ));
    s.push_str(&rect(
        56.0,
        y + 34.0,
        34.0,
        17.0,
        &t.chrome.hardware.shade(-0.40).css(),
    ));
    s.push_str(&label(
        62.0,
        y + 47.0,
        10.0,
        &t.chrome.hardware_mark.shade(-0.1).css(),
        "FX",
    ));
    // The input combo: no component, so a rectangle and a caret.
    s.push_str(&rect(
        94.0,
        y + 34.0,
        186.0,
        17.0,
        &t.chrome.hardware.shade(-0.40).css(),
    ));
    s.push_str(&label(
        100.0,
        y + 47.0,
        11.0,
        &t.chrome.hardware_mark.shade(0.5).css(),
        "Input 1",
    ));
    s.push_str(&format!(
        "<path d=\"M {} {} h 7 l -3.5 4 z\" fill=\"{}\"/>",
        266.0,
        y + 40.0,
        t.chrome.hardware_mark.shade(0.2).css()
    ));

    // Mute and solo live outside the tint, stacked.
    s.push_str(&rect(
        296.0,
        y,
        44.0,
        71.0,
        &t.chrome.hardware.shade(-0.40).css(),
    ));
    s.push_str(&at(
        306.0,
        y + 4.0,
        &mute(
            art("track_mute_on"),
            (1.0 / 24.0, 20.0 / 24.0),
            true,
            tk.muted,
        ),
    ));
    s.push_str(&at(
        306.0,
        y + 36.0,
        &solo(
            art("track_solo_on"),
            (1.0 / 24.0, 20.0 / 24.0),
            true,
            tk.soloed,
        ),
    ));
    s
}

fn mixer_strip(x: f32, n: u32, tk: &Track, h: f32) -> String {
    let t = daw_theme::Theme::default();
    let g = |v: f32| t.chrome.hardware.shade(v);
    let mut s = String::new();

    // Sections from `rtconfig.txt`, at scale 1 in wide mode, and they are
    // a *function of height* — which is the thing to know about this
    // panel. `mcp_w` is 86 and `mcp_h` 371, but:
    //
    //     fx_sec  33 always
    //     pan_sec  h < hide_pan   (260) ?  6 : 33
    //     in_sec   h < hide_input (350) ? 22 : h < hide_inputFX (400) ? 42 : 54
    //     bot_sec 47, of which label_sec is 26
    //     stretch  whatever is left
    //
    // and the track tint, `mcp.custom.bg`, is `pan_sec` extended by
    // `in_sec`'s height. At REAPER's docked 235 that is 6 + 22 = 28 rows,
    // which is exactly the band the screenshot shows — the mixer there is
    // in its compressed form, not its full one.
    const W: f32 = 86.0;
    let fx_h = 33.0f32;
    let pan_h = if h < 260.0 { 6.0 } else { 33.0 };
    let in_h = if h < 350.0 {
        22.0
    } else if h < 400.0 {
        42.0
    } else {
        54.0
    };
    let bot_h = 47.0f32;
    let pan_y = fx_h;
    let stretch_y = fx_h + pan_h + in_h;
    let bot_y = h - bot_h;
    let stretch_h = bot_y - stretch_y;

    s.push_str(&rect(x, 0.0, W, h, &g(-0.40).css()));
    // The full `mcp.custom.bg`: `pan_sec` extended by `in_sec`'s height,
    // 6 + 22 at this height, running rows 33 to 61.
    //
    // I cut this to 14 on one column's evidence and it was wrong. Column
    // 62 crosses the record arm's *housing*, so the dark it turns at row
    // 47 is the housing's crown, not the band's foot — scanning x 8, 30
    // and 40, all clear of the arm, gives `a8415c` solid to 61. One
    // column is not a measurement when something else is drawn over it.
    // One row deeper than `pan_sec + in_sec`: the sections total 28 and
    // the band measures 29, running 33 to 61 inclusive against a body
    // that starts at 62. The extra row is the `lscale` term in
    // `mcp.custom.bg`'s expression, which this does not evaluate — so it
    // is added here as the measurement rather than derived.
    s.push_str(&rect(x, pan_y, W, stretch_y - pan_y + 1.0, &tk.tint.css()));
    // `mcp.custom.bg_hl_t` — one row of `hl_color` along the tint's top
    // edge. REAPER prints `b0526b` there against the band's `a8415c`, and
    // without it the band starts flat where the source starts lit.
    s.push_str(&rect(x, pan_y, W, 1.0, &tk.tint.shade(0.13).css()));

    // fx_sec holds the FX button and its bypass, not a list: `mcp.fx` is
    // [7 7 43 20] of the section and `mcp.fxbyp` [0 0 29 20] butted onto
    // its right.
    //
    // Drawn *into* those boxes it came out badly stretched — the pill's
    // art is 28 wide by 22 and the boxes are 43 by 20, so a 1.54 stretch
    // across and a 0.91 squash down. That elongated the rounded ends into
    // a notch and blew the `FX` up to half again its size. REAPER gets
    // the width by nine-slicing: only the flat middle grows and the ends
    // stay 1:1, which is why its label reads small in a long pill.
    //
    // Nothing here can nine-slice a nested `<svg>`, so both halves are
    // drawn at their own size instead. The button is shorter than
    // REAPER's and the right shape, which is the better trade — matching
    // the width wants nine-slice support in this layer, not a scale.
    s.push_str(&at(
        x + 4.0,
        6.0,
        &render_svg(
            v::FxControl,
            v::FxControlProps {
                pane: None,
                part: v::FxPart::Label,
                // 43 and 29 — `mcp.fx` and `mcp.fxbyp` — reached by
                // growing the pill's middle, not by scaling it.
                chain: v::FxChain::Empty,
                bypass: v::FxBypass::Empty,
                family: v::FxFamily::Mixer,
                width: Some(43),
                height: Some(22),
                at: v::Interaction::Normal,
            },
        ),
    ));
    // 49, not 50. `mcp.fxbyp` starts exactly where `mcp.fx` ends, but the
    // toggle's own art leaves a seam column at its left — that is what
    // `leading_gap` exists for on the export side — so placed at the
    // arithmetic join the two halves show a bare pixel between them.
    // 46, so the toggle covers the gutter column. The gutter is real in
    // the art — one empty column between the two blits — but REAPER's
    // output has no gap there, so leaving it open showed the strip
    // through the middle of the pill.
    s.push_str(&at(
        x + 46.0,
        6.0,
        &render_svg(
            v::FxControl,
            v::FxControlProps {
                pane: None,
                part: v::FxPart::Toggle,
                chain: v::FxChain::Empty,
                bypass: v::FxBypass::Empty,
                family: v::FxFamily::Mixer,
                width: Some(28),
                height: Some(22),
                at: v::Interaction::Normal,
            },
        ),
    ));

    // Compressed, `mcp.pan` is re-anchored off `mcp.recmode` and both it
    // and the record arm sit inside the band — pan at its left, arm at
    // its right, which is where the shot has them.
    // Both are placed on the *centres* the shot gives, not on a corner:
    // scanning row 565 across a strip puts the pan cap on local x 22 and
    // the arm's hole on 67, and the arm's ring sits at 0.486 of its own
    // 36-wide cell rather than in the middle of it. So the knob starts at
    // 9.5 and the arm at 49.5 — which also lines the arm up with the
    // monitor, mute and solo column beneath it, whose centre is 66.5.
    s.push_str(&at(
        x + 10.0,
        pan_y + 0.5,
        &render_svg(
            v::PanningKnob,
            v::PanProps {
                position: 0.0,
                large: false,
                indicator: true,
                width: NONE.0,
                height: NONE.1,
            },
        ),
    ));
    // Everything in the right-hand column shares one axis and one chain
    // of offsets, which is how `rtconfig` writes it:
    //
    //     mcp.recmon = mcp.recarm + [7 20 21 20]
    //     mcp.mute   = mcp.recmon + [0 19 21 20]
    //     mcp.solo   = mcp.mute   + [0 21 21 20]
    //     mcp.io     = mcp.solo   + [-1 23 23 30]
    //
    // Each is anchored to the last, so placing them at absolute offsets
    // from the section — which is what this did — lets them drift apart
    // one at a time. `COL` is the axis they centre on; the arm's ring
    // sits at 0.486 of its 36-wide cell rather than in the middle, so it
    // is the one that cannot be centred by halving its width.
    const COL: f32 = 66.0;
    // The arm sits low enough that the tint's foot crosses its housing at
    // the *flares* rather than at the straight sides below them. The
    // housing's base corners run 45 degrees out between 0.592 and 0.717
    // of its 24-row cell — rows 14.2 to 17.2 — so the boundary has to
    // land in there. Three rows higher and the tint cut across the
    // vertical sides, which reads as two upright lines coming out of the
    // colour instead of a shape emerging from it.
    let arm_y = pan_y + 12.3;
    s.push_str(&at(
        x + COL - 36.0 * 0.486,
        arm_y,
        &render_svg(
            v::RecordArmButton,
            v::RecordArmProps {
                state: if tk.armed {
                    v::RecordArm::On
                } else {
                    v::RecordArm::Off
                },
                art: art("mcp_recarm_on"),
                housing: true,
                width: NONE.0,
                height: NONE.1,
                at: v::Interaction::Normal,
            },
        ),
    ));
    // The offsets `rtconfig` writes — 20, 19, 21, 23 — are the *nominal*
    // ones: every line in that chain also carries `+ [0 padding]`, and
    // the padding is what actually spaces them. Scanning column 62 of a
    // strip gives the rows REAPER lands on — arm 45, monitor 70, mute 88,
    // solo 111, io 136 — which is 25, 18, 23, 25. Using the nominal
    // numbers put the routing button eight rows high, and it is the one
    // furthest down the chain so it collects every error above it.
    let mon_y = arm_y + 25.0;
    let mute_y = mon_y + 18.0;
    let solo_y = mute_y + 23.0;
    let io_y = solo_y + 25.0;
    s.push_str(&at(
        x + COL - 10.5,
        mon_y,
        &render_svg(
            v::InputMonitorIndicator,
            v::MonitoringProps {
                state: if tk.armed {
                    v::Monitoring::On
                } else {
                    v::Monitoring::Off
                },
                art: art("mcp_monitor_on"),
                axis: v::Axis::Vertical,
                width: NONE.0,
                height: NONE.1,
                at: v::Interaction::Normal,
            },
        ),
    ));
    s.push_str(&at(
        x + COL - 10.5,
        mute_y,
        &mute(art("mcp_mute_on"), (0.0, 1.0), false, tk.muted),
    ));
    s.push_str(&at(
        x + COL - 10.5,
        solo_y,
        &solo(art("mcp_solo_on"), (0.0, 1.0), false, tk.soloed),
    ));
    s.push_str(&at(
        x + COL - 11.5,
        io_y,
        &render_svg(
            v::RoutingButton,
            v::RoutingProps {
                // Unlit: the reference track has no sends, and lighting
                // the lane put a yellow bar in the button where REAPER
                // shows grey.
                has_sends: false,
                has_receives: false,
                disabled: false,
                axis: v::Axis::Vertical,
                art: IO_DRAWN_SHORT,
                width: NONE.0,
                height: NONE.1,
                at: v::Interaction::Normal,
            },
        ),
    ));

    // stretch_sec, across: `mcp.meter` is [4 4 22 -4] and two wider in
    // wide mode, so the meter block is x 4..28 — and that block is the
    // *scale as well as the bars*, which is why the numbers have nowhere
    // else to go. Fader 28..49, button column 55..76.
    let scale_h = stretch_h - 8.0;
    s.push_str(&meter(
        x + 4.0,
        stretch_y + 4.0,
        24.0,
        scale_h,
        vec![0.0, 0.0],
        &["-inf", "-6-", "-18-", "-30-", "-42-", "-54-"],
    ));
    s.push_str(&at(
        x + 28.0,
        stretch_y + 4.0,
        &render_svg(
            v::VolumeFaderTrack,
            // The whole rail as one pane. The contact sheet draws the art
            // at its source proportions to photograph it, not to build a
            // strip — a stretched fader is `slice::NamedArt::stack`, which
            // is the panel's job and not this sheet's.
            v::FaderCapProps {
                accent: None,
                pane: None,
                width: Some(21),
                height: Some(scale_h as u32),
            },
        ),
    ));
    s.push_str(&at(
        x + 28.0,
        stretch_y + 22.0,
        &render_svg(
            v::VolumeFaderCap,
            v::FaderCapProps {
                accent: None,
                pane: None,
                width: Some(21),
                height: Some(44),
            },
        ),
    ));

    // bot_sec: the name over the number band.
    s.push_str(&label(
        x + 26.0,
        bot_y + 17.0,
        11.0,
        &t.chrome.hardware_mark.shade(0.5).css(),
        tk.name,
    ));
    // 27, not 26: REAPER's number band starts at row 215 against a body
    // that runs to 214, and ours began a row early — which showed as a
    // grey line where the red should already have started. It carries the
    // same `hl_color` top row the track tint does.
    s.push_str(&rect(x + 1.0, bot_y + 27.0, W - 2.0, 20.0, &tk.tint.css()));
    s.push_str(&rect(
        x + 1.0,
        bot_y + 27.0,
        W - 2.0,
        1.0,
        &tk.tint.shade(0.13).css(),
    ));
    s.push_str(&label(
        x + 40.0,
        bot_y + 41.0,
        11.0,
        "#f0f0f0",
        &n.to_string(),
    ));
    s
}

fn main() {
    let t = daw_theme::Theme::default();
    let hex = |r, g, b| Color::rgb(r, g, b);
    let tracks = [
        Track {
            name: "Kick",
            tint: hex(0xa6, 0x3a, 0x56),
            armed: true,
            muted: false,
            soloed: false,
        },
        Track {
            name: "Snare",
            tint: hex(0xa6, 0x3a, 0x56),
            armed: true,
            muted: false,
            soloed: true,
        },
        Track {
            name: "OH",
            tint: hex(0xa6, 0x3a, 0x56),
            armed: false,
            muted: false,
            soloed: false,
        },
        Track {
            name: "Bass",
            tint: hex(0x3c, 0x5f, 0x9e),
            armed: false,
            muted: true,
            soloed: false,
        },
        Track {
            name: "Gtr",
            tint: hex(0x2e, 0x8e, 0xc4),
            armed: false,
            muted: false,
            soloed: false,
        },
    ];

    let (w, h) = (760.0f32, 660.0f32);
    // #333333, sampled from the arrange background. `surface` is
    // #3e3e3e, which is the *toolbar*, and using it made every
    // ground in this panel eleven levels light.
    let ground = t.chrome.hardware.shade(-0.19);
    let mut body = rect(0.0, 0.0, w, h, &ground.css());
    for (i, tk) in tracks.iter().enumerate() {
        body.push_str(&track_row(i as f32 * 71.0, i as u32 + 1, tk));
    }
    // The transport, in the bar under the track panel, as REAPER has it.
    body.push_str(&rect(
        0.0,
        360.0,
        w,
        34.0,
        &t.chrome.hardware.shade(-0.38).css(),
    ));
    for (i, glyph) in [
        v::TransportGlyph::Home,
        v::TransportGlyph::End,
        v::TransportGlyph::Stop,
        v::TransportGlyph::Play,
        v::TransportGlyph::Pause,
        v::TransportGlyph::Record,
    ]
    .into_iter()
    .enumerate()
    {
        body.push_str(&at(
            10.0 + i as f32 * 36.0,
            364.0,
            &render_svg(
                v::TransportButton,
                v::TransportProps {
                    glyph,
                    on: glyph == v::TransportGlyph::Play,
                    cell: (36.0, 26.0),
                    width: NONE.0,
                    height: NONE.1,
                    at: v::Interaction::Normal,
                },
            ),
        ));
    }
    body.push_str(&at(
        238.0,
        365.0,
        &render_svg(
            v::TransportButton,
            v::TransportProps {
                glyph: v::TransportGlyph::Repeat,
                on: false,
                cell: (32.0, 24.0),
                width: NONE.0,
                height: NONE.1,
                at: v::Interaction::Normal,
            },
        ),
    ));
    body.push_str(&rect(
        270.0,
        365.0,
        190.0,
        24.0,
        &t.chrome.hardware.shade(-0.40).css(),
    ));
    body.push_str(&label(
        280.0,
        381.0,
        13.0,
        &t.chrome.hardware_mark.shade(0.5).css(),
        "1.1.00 / 0:00.000",
    ));

    // And the mixer along the bottom.
    body.push_str(&rect(0.0, 400.0, w, 245.0, &ground.css()));
    for (i, tk) in tracks.iter().enumerate() {
        // REAPER's docked mixer measures 235 in the reference shot.
        let strip = mixer_strip(i as f32 * 88.0 + 4.0, i as u32 + 1, tk, 235.0);
        body.push_str(&format!("<g transform=\"translate(0 400)\">{strip}</g>"));
    }

    let doc = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" \
         viewBox=\"0 0 {w} {h}\">{body}</svg>"
    );

    let out = std::path::Path::new("target/theme-shots");
    std::fs::create_dir_all(out).unwrap();
    std::fs::write(out.join("native-panels.svg"), &doc).unwrap();
    // Twice the art's size, because the point is that it scales.
    match rasterise(&doc, (w * 2.0) as u32, (h * 2.0) as u32) {
        Ok(img) => {
            img.save(out.join("native-panels.png")).unwrap();
            println!("wrote {}", out.join("native-panels.png").display());
        }
        Err(e) => eprintln!("rasterise failed: {e}"),
    }
}
