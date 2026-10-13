//! The mixer controls as true vectors.
//!
//! [`crate::mixer_controls`] draws these from traced rects — pixel-exact
//! with the originals, and pixel-*shaped*: zoom in and you see the steps,
//! because a trace is a picture of a bitmap however you store it.
//!
//! These are the same controls drawn as shapes — circles, rounded rects,
//! gradients, glyphs — so they stay sharp at any zoom. Proportions are
//! taken from the originals (a ring for record-arm, a gradient-filled
//! rounded rect with a bevelled letter for mute/solo/FX, a bevelled body
//! with a ribbed panel for the fader cap), so they still read as the same
//! theme.
//!
//! # Everything is proportional
//!
//! Each control draws into a `viewBox` in its own units and every dimension
//! is a fraction of that — no pixel constants. That is what "infinitely
//! zoomable" actually requires: a 1px border baked in at 20px tall becomes
//! a 10px slab at 400px tall.
//!
//! # The SVG id rule
//!
//! SVG ids are **document-global**, and a mixer is a row of strips: every
//! inline `<svg>` in a page shares one id namespace, and a `url(#…)`
//! reference resolves to the *first* element carrying that id, whichever
//! strip it came from. The meter learned this the hard way — its clip
//! regions varied per strip, and every strip's scale clipped to the first
//! strip's level.
//!
//! So the rule is: **ids need distinguishing exactly when their content
//! varies per instance.** A gradient built only from `Theme::default()`
//! renders identically on every strip, so resolving to the first copy
//! changes nothing and a plain id is fine. The moment a def's content
//! depends on a prop — a state, a colour, a cell size, a hover — two
//! instances can disagree, and the first one wins silently.
//!
//! The mechanism here is [`vary`]: derive the id *from the varying
//! content*, so equal content shares an id harmlessly and different
//! content cannot collide. No tag prop to thread, no call site to update,
//! and forgetting a new dependency shows up as a wrong id the moment two
//! states differ.

use daw_theme::{Color, Theme};
use dioxus::prelude::*;

pub use crate::mixer_controls::{FxChain, Interaction, Monitoring, RecordArm, Solo};
pub use crate::slice::NamedArt;

/// An id for a def whose content varies per instance — see the SVG id rule
/// in the module docs.
///
/// The id is the base followed by the varying inputs, stripped to the
/// characters an id may carry. Pass *every* input the def's content is
/// built from; one that is missed is the meter bug waiting for the first
/// pair of instances that differ only in it.
fn vary(base: &str, parts: &[&str]) -> String {
    let mut id = String::from(base);
    for part in parts {
        id.extend(part.chars().filter(|c| c.is_ascii_alphanumeric()));
    }
    id
}

/// Which way a control is laid out.
///
/// The track panel and the mixer draw the *same* controls along different
/// axes — routing stacks its three lanes in the mixer and sets them side
/// by side in the track panel; input monitoring radiates downward there
/// and rightward here. Same geometry, turned a quarter turn, so it is a
/// prop rather than a second component.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Axis {
    /// Mixer: lanes stacked, waves radiating down.
    #[default]
    Vertical,
    /// Track panel: lanes in a row, waves radiating right.
    Horizontal,
}

/// Common sizing props.
#[derive(Props, Clone, PartialEq, Default)]
pub struct VectorProps {
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    /// Pointer state — hover lifts the face, pressed sinks it.
    #[props(default)]
    pub at: Interaction,
}

/// Deepen a colour without dimming it.
///
/// Holds the strongest channel and pulls the others down, which is what
/// ReaperTips' lit buttons actually do from top to bottom: solo runs
/// #d3a738 to #d0943a — red pinned at 210, green falling 167 to 148 —
/// and defeat holds its blue at 211 while red drops away.
///
/// `shade` cannot express that. It mixes toward black, so it scales all
/// three channels together and takes the dominant one with it: the face
/// loses chroma as it descends and the middle of the button reads dull
/// against the original, which is exactly how this was first noticed.
fn deepen(c: Color, amount: f32) -> Color {
    let peak = c.r.max(c.g).max(c.b);
    // A grey has no non-dominant channel to pull, so holding the peak
    // would leave it untouched — which silently cost every *unlit* button
    // both its gradient and its pressed state, since those faces are
    // neutral. There, deepening is just darkening.
    if peak == c.r.min(c.g).min(c.b) {
        return c.shade(-amount);
    }
    let pull = |v: u8| {
        if v == peak {
            v
        } else {
            (v as f32 * (1.0 - amount)).round().clamp(0.0, 255.0) as u8
        }
    };
    Color::rgb(pull(c.r), pull(c.g), pull(c.b))
}

/// Brighten a colour by scaling its channels, clamping at the top.
///
/// The counterpart to [`deepen`], and what a hover is here: solo goes
/// #d29e37 to #ffdb59 under the pointer — the same colour turned up, red
/// running into the ceiling while green and blue climb.
///
/// Three models were measured against the art. A wash toward white
/// (`shade`) moves it a fifth as far and leaves every lit button barely
/// changed when hovered. An HSL lightness lift looks right on paper —
/// the source's three buttons all rise about 14 points — but holding
/// saturation while lightness climbs desaturates in RGB, and defeat's
/// hover *gains* saturation in the source. Scaling the channels lands
/// solo and defeat within a few points each; mute's red runs about 8%
/// hot, which is the price of one rule for three buttons.
fn lift(c: Color, amount: f32) -> Color {
    let up = |v: u8| (v as f32 * (1.0 + amount)).round().clamp(0.0, 255.0) as u8;
    Color::rgb(up(c.r), up(c.g), up(c.b))
}

/// A control's palette, resolved once per render.
struct Ink {
    face: Color,
    border: Color,
    text: Color,
}

/// `sinks` — does pressing darken the face?
///
/// In the mixer it does: `mcp_mute_off` goes #3f3f3f to #363636. In the
/// track panel it does *not* — `track_mute_off` shows #494949 in both
/// cells, identical. Assuming either way invents a state one of the two
/// families does not have.
fn ink(lit: Option<Color>, at: Interaction, sinks: bool, hover: f32) -> Ink {
    let t = Theme::default();
    let c = &t.chrome;
    // Unlit controls take the neutral control grey, not a shade of the
    // surface ladder: deriving it from `surface_raised` made every mute,
    // solo and FX button blue-cast and far darker than the art it stands
    // in for.
    let base = lit.unwrap_or(c.hardware);
    // Hover is far stronger than the 12% wash this used: solo lifts to
    // #ffdb59, with red clipping at the ceiling.
    //
    // How far is per button, because the source's is. Measured as the
    // per-channel ratio from normal to hover:
    //
    //     mute    x1.25  x1.56  x1.52
    //     solo    x1.21  x1.39  x1.62
    //     defeat  x1.85  x1.43  x1.21
    //
    // Mute's is the gentlest and the flattest; a single amount tuned to
    // solo drove mute's red to 238 against the source's 220.
    let face = match at {
        Interaction::Normal => base,
        Interaction::Hover => lift(base, hover),
        Interaction::Pressed if sinks => deepen(base, 0.12),
        Interaction::Pressed => base,
    };
    Ink {
        // The originals light from the top. The lighter stop is derived at
        // the point of use rather than carried here, so each control can
        // pick its own falloff.
        face,
        border: c.hardware_edge,
        // Neutral, and the same lit or not: the source prints #cccccc on
        // mute and solo in both states. `text` is the panel blue-white,
        // which on a plastic button reads as a backlit legend.
        text: c.hardware_mark.shade(0.26),
    }
}

// ── a labelled button: mute, solo, FX ────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
pub struct LabelButtonProps {
    /// Where the button sits in its cell: `(top, height)` as fractions.
    ///
    /// The mixer's buttons fill their cell; the track panel's are inset a
    /// row at the top and three at the bottom — `track_mute_on` draws rows
    /// 1..20 of 24, and the magenta guide runs to row 20 to say so.
    /// Filling the cell there pushed the button past its own guide.
    #[props(default = (0.0, 1.0))]
    pub body: (f32, f32),
    /// Draw the soft row under the button.
    ///
    /// Every track-panel label has one except the lit mute, which stops
    /// dead at its bottom border. That is the source's own inconsistency,
    /// not a rule about lit buttons — the lit solo and the blue defeat
    /// both keep theirs — so it has to be told rather than derived.
    #[props(default = true)]
    pub shadow: bool,
    pub label: String,
    /// The printed letter's colour.
    ///
    /// Brighter in the track panel than in the mixer — #f2f2f2 against
    /// #cccccc — which is not a shade of one value but two different
    /// choices, so it comes from the caller rather than from `ink`.
    #[props(default)]
    pub legend: Option<Color>,
    /// Does pressing darken the face? See [`ink`].
    #[props(default = true)]
    pub sinks: bool,
    /// How far the face lifts under the pointer — see [`ink`].
    #[props(default = 0.35)]
    pub hover: f32,
    /// How far the face deepens from top to bottom — see [`deepen`].
    ///
    /// Not shared, because the source does not share it: mute falls 15%
    /// in its non-dominant channels over the button's height, solo 11%,
    /// and blue-defeat more still.
    #[props(default = 0.15)]
    pub depth: f32,
    /// Darken by scaling every channel rather than holding the strongest.
    ///
    /// Solo and blue-defeat hold theirs — solo's red sits at 210 top and
    /// bottom while its green falls and its blue actually *rises* — which
    /// is what [`deepen`] does. Mute does not: its face runs 184,58,78 to
    /// 164,51,70, a clean 0.89 on all three. Read as a `deepen` its red
    /// barely moved and the button came out flat and light.
    #[props(default = false)]
    pub scales: bool,
    /// Face colour when engaged. `None` draws the resting state.
    #[props(default)]
    pub lit: Option<Color>,
    /// The REAPER image this draws: the box its art is drawn at, and which
    /// band of that box takes up slack when it is drawn bigger — see
    /// [`NamedArt`]. Deliberately without a default, because one component
    /// serves several images at several sizes.
    pub art: NamedArt,
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    /// Pointer state — hover lifts the face, pressed sinks it.
    #[props(default)]
    pub at: Interaction,
}

/// The shape mute, solo and FX all share.
///
/// Measured off `mcp_mute_on`, cell 0: 21x20, a 1px near-black border, a
/// single lighter highlight row just inside the top, and a body gradient
/// running darker downward. The glyph is about 9px tall — under half the
/// cell — in a soft off-white, with **no drop shadow**; the darker pixels
/// around it in the original are antialiasing, not a stamped edge.
///
/// The first version of this had a 0.62-of-height glyph and a hard
/// offset shadow, which read as a bevel at 20px and as a badge at 300px.
#[component]
pub fn LabelButton(props: LabelButtonProps) -> Element {
    let k = ink(props.lit, props.at, props.sinks, props.hover);
    let (vw, vh) = props.art.source;
    let (body_y, body_h) = (vh * props.body.0, vh * props.body.1);
    // The radius was never the problem — the stroke was.
    //
    // With a stroked border the corner looked far too round, and shrinking
    // the radius to a pixel was the obvious fix. It was the wrong one: a
    // stroke covers only half its path, so what actually showed at the
    // corner was the *face* bleeding past it. With a filled frame the
    // original 0.10 of height lands within a few points of the source at
    // every corner pixel.
    let r = vh * 0.10;
    // Exactly one pixel, offset half of one, so the border lands inside a
    // single row rather than straddling two. At `vh * 0.05` it was 1.2px
    // centred on an integer: the bottom row came out a blend of border
    // and face — #832d3b where the source still has body colour — and the
    // whole button read a row short.
    let edge = 1.0f32;
    let floor = if props.scales {
        lift(k.face, -props.depth)
    } else {
        deepen(k.face, props.depth)
    }
    .css();
    // The label alone is not enough: two Mute buttons in one document, one
    // lit, have the same label and different faces.
    let id = vary("lb", &[&props.label, &k.face.css(), &floor]);

    rsx! {
        svg {
            width: "{props.width.unwrap_or(vw as u32)}",
            height: "{props.height.unwrap_or(vh as u32)}",
            view_box: "0 0 {vw} {vh}",
            xmlns: "http://www.w3.org/2000/svg",
            defs {
                linearGradient { id: "{id}", x1: "0", y1: "0", x2: "0", y2: "1",
                    // The face *starts* at the button's own colour —
                    // mute's top row is 184,58,78 and `signal.mute` is
                    // #b8394e, the same value. The 6% lift on top of that
                    // was doubling up with the highlight row below it.
                    stop { offset: "0", stop_color: "{k.face.css()}" }
                    stop { offset: "1", stop_color: "{floor}" }
                }
            }
            // A soft row under the button, where the cell leaves room for
            // one. The track panel's labels sit in a 24-row cell with a
            // 20-row body, and the spare row at the bottom is not empty —
            // it carries the button's shadow at about a third alpha. The
            // mixer's fill their cell edge to edge and get none, which is
            // why this keys off the geometry rather than the family.
            if props.shadow && body_y + body_h < vh - 0.5 {
                rect {
                    x: "{r}", y: "{body_y + body_h}",
                    width: "{vw - r * 2.0}", height: "1",
                    fill: "{k.border.css()}", fill_opacity: "0.31",
                }
            }
            // Inset by half the border so the stroke sits inside.
            // Border as a *filled* shape with the face inset on top of
            // it, not a stroke around the face.
            //
            // A stroke is centred on the path it follows, so it covers
            // only half of the fill's antialiased edge — and at a rounded
            // corner the rest of that edge shows past it. The corner
            // pixels came out a blend of border and face (R=54 where the
            // source is a clean R=23 at reduced alpha), which reads as
            // the colour leaking out of the corner rather than being held
            // by the frame. Filled and inset, the face cannot reach the
            // outside: there is a solid pixel of border in the way.
            rect {
                x: "0", y: "{body_y}",
                width: "{vw}", height: "{body_h}",
                rx: "{r}",
                fill: "{k.border.css()}",
            }
            rect {
                x: "{edge}", y: "{body_y + edge}",
                width: "{vw - edge * 2.0}", height: "{body_h - edge * 2.0}",
                rx: "{(r - edge).max(0.0)}",
                fill: "url(#{id})",
            }
            // The lit row immediately inside the top border — exactly one
            // row, at y=2 of the source's 1..20 button. A 1.2px band
            // starting at 2.24 spilled into row 3 and left row 2 dark,
            // which put the highlight a row low.
            rect {
                x: "{vw * 0.1}", y: "{body_y + 1.0}",
                width: "{vw * 0.8}", height: "1.0",
                fill: "{k.face.shade(0.22).css()}",
                fill_opacity: "0.9",
            }
            text {
                x: "{vw * 0.5}",
                // Above the geometric centre, deliberately. The source
                // glyph occupies rows 6..13 of a 20-row cell — centred on
                // 9.5, not 10 — and `dominant-baseline: central` centres on
                // the font's own middle, which put it a further half-pixel
                // down. Sitting it at 0.54 rendered a full row low in every
                // one of mute, solo and FX.
                y: "{body_y + body_h * 0.515}",
                text_anchor: "middle", dominant_baseline: "central",
                font_family: "Fira Sans, DejaVu Sans, sans-serif",
                // Heavier and larger than the measured glyph height: at
                // 21x20 a normal-weight 9px letter rasterises thin and
                // grey, where the original is crisp and solid. Matching
                // the *measured* size gave a lighter button than the
                // original, which is the trap in measuring geometry
                // without checking how it renders.
                font_weight: "900",
                // Scaled with the *cell*, which is not obviously right —
                // both families draw a 20-row body and only the cell
                // differs, 20 against 24 — but sizing off the body made
                // both track-panel labels measurably worse. The source's
                // track-panel letters really are the larger pair.
                font_size: "{vh * 0.58}",
                fill: "{props.legend.unwrap_or(k.text).css()}",
                "{props.label}"
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct ToggleProps {
    /// The resting face, when nothing is engaged.
    ///
    /// Not `hardware` in either family, and not the same in both: the
    /// mixer's unlit button runs #464646 down to #3e3e3e and the track
    /// panel's #4e4e4e down to #444444. Taking the plain hardware grey
    /// left the mixer eight levels dark and the track panel sixteen.
    #[props(default)]
    pub unlit: Option<Color>,
    /// Does pressing darken the face? See [`ink`].
    #[props(default = true)]
    pub sinks: bool,
    /// How far the face lifts under the pointer — see [`ink`].
    #[props(default = 0.35)]
    pub hover: f32,
    /// How far the face darkens — see [`LabelButtonProps::depth`].
    #[props(default = 0.14)]
    pub depth: f32,
    /// The printed letter's colour — see [`LabelButtonProps::legend`].
    #[props(default)]
    pub legend: Option<Color>,
    /// Where the button sits in its cell: `(top, height)` as fractions.
    ///
    /// The mixer's buttons fill their cell; the track panel's are inset a
    /// row at the top and three at the bottom — `track_mute_on` draws rows
    /// 1..20 of 24, and the magenta guide runs to row 20 to say so.
    /// Filling the cell there pushed the button past its own guide.
    #[props(default = (0.0, 1.0))]
    pub body: (f32, f32),
    #[props(default)]
    pub on: bool,
    /// The REAPER image this draws: the box its art is drawn at, and which
    /// band of that box takes up slack when it is drawn bigger — see
    /// [`NamedArt`]. Deliberately without a default, because one component
    /// serves several images at several sizes.
    pub art: NamedArt,
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    /// Pointer state — hover lifts the face, pressed sinks it.
    #[props(default)]
    pub at: Interaction,
}

#[component]
pub fn MuteButton(props: ToggleProps) -> Element {
    let t = Theme::default();
    rsx! {
        LabelButton {
            label: "M",
            lit: props.on.then_some(t.signal.mute).or(props.unlit),
            art: props.art, body: props.body, legend: props.legend,
            depth: props.depth, sinks: props.sinks, hover: props.hover,
            shadow: !props.on, scales: true,
            width: props.width, height: props.height, at: props.at,
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct SoloProps {
    /// The resting face, when nothing is engaged.
    ///
    /// Not `hardware` in either family, and not the same in both: the
    /// mixer's unlit button runs #464646 down to #3e3e3e and the track
    /// panel's #4e4e4e down to #444444. Taking the plain hardware grey
    /// left the mixer eight levels dark and the track panel sixteen.
    #[props(default)]
    pub unlit: Option<Color>,
    /// Does pressing darken the face? See [`ink`].
    #[props(default = true)]
    pub sinks: bool,
    /// How far the face lifts under the pointer — see [`ink`].
    #[props(default = 0.35)]
    pub hover: f32,
    /// How far the face darkens — see [`LabelButtonProps::depth`].
    #[props(default = 0.14)]
    pub depth: f32,
    /// The printed letter's colour — see [`LabelButtonProps::legend`].
    #[props(default)]
    pub legend: Option<Color>,
    /// Where the button sits in its cell: `(top, height)` as fractions.
    ///
    /// The mixer's buttons fill their cell; the track panel's are inset a
    /// row at the top and three at the bottom — `track_mute_on` draws rows
    /// 1..20 of 24, and the magenta guide runs to row 20 to say so.
    /// Filling the cell there pushed the button past its own guide.
    #[props(default = (0.0, 1.0))]
    pub body: (f32, f32),
    #[props(default)]
    pub state: Solo,
    /// The REAPER image this draws: the box its art is drawn at, and which
    /// band of that box takes up slack when it is drawn bigger — see
    /// [`NamedArt`]. Deliberately without a default, because one component
    /// serves several images at several sizes.
    pub art: NamedArt,
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    /// Pointer state — hover lifts the face, pressed sinks it.
    #[props(default)]
    pub at: Interaction,
}

#[component]
pub fn SoloButton(props: SoloProps) -> Element {
    let t = Theme::default();
    // Defeat is a different thing from solo, not more of it, so it gets a
    // different hue rather than a brighter one.
    let lit = match props.state {
        // `props.unlit`, not `None`. Dropped, this fell back to the plain
        // hardware grey and the resting face came out sixteen levels dark
        // in the track panel and eight in the mixer — which is precisely
        // what the prop's own doc comment says it exists to prevent.
        // `MuteButton` has always passed it through; solo never did.
        Solo::Off => props.unlit,
        Solo::On => Some(t.signal.solo),
        // #3898d3 in the source — its own blue, a clear step below the
        // accent used for a lit routing lane rather than the same one.
        // At -0.22 it lost too much blue (198 against 211).
        Solo::Defeat => Some(t.chrome.accent.shade(-0.18)),
    };
    rsx! {
        LabelButton {
            label: "S", lit, art: props.art, body: props.body,
            legend: props.legend, depth: props.depth, sinks: props.sinks,
            hover: props.hover,
            width: props.width, height: props.height, at: props.at,
        }
    }
}

/// What the FX chain is doing, as the bypass toggle reports it.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum FxBypass {
    /// No FX at all — the toggle is an affordance, not a state.
    #[default]
    Empty,
    /// Chain active.
    On,
    /// Chain bypassed.
    Off,
}

#[derive(Props, Clone, PartialEq)]
pub struct FxProps {
    #[props(default)]
    pub state: FxChain,
    #[props(default)]
    pub family: FxFamily,
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    /// Pointer state — hover lifts the face, pressed sinks it.
    #[props(default)]
    pub at: Interaction,
}

/// The labelled half of [`FxControl`], as its own image.
#[component]
pub fn FxButton(props: FxProps) -> Element {
    rsx! {
        FxControl {
            chain: props.state,
            family: props.family,
            part: FxPart::Label,
            width: props.width,
            height: props.height,
            at: props.at,
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct FxBypassProps {
    #[props(default)]
    pub state: FxBypass,
    #[props(default)]
    pub family: FxFamily,
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    /// Pointer state — hover lifts the face, pressed sinks it.
    #[props(default)]
    pub at: Interaction,
}

/// The lit half of [`FxControl`], as its own image.
#[component]
pub fn FxBypassToggle(props: FxBypassProps) -> Element {
    rsx! {
        FxControl {
            bypass: props.state,
            family: props.family,
            part: FxPart::Toggle,
            width: props.width,
            height: props.height,
            at: props.at,
        }
    }
}

// ── FX: one pill, blitted as two images ──────────────────────────────────

/// Which panel's FX control this is.
///
/// The two are the same control at different sizes with different
/// materials, and every dimension differs, so the table below is the one
/// place those numbers live.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum FxFamily {
    /// `mcp_fx_*` + `track_fx*_v`: opaque plastic, 28 + 18 wide.
    #[default]
    Mixer,
    /// `track_fx_*` + `track_fx*_h`: a translucent scrim, 20 + 16 wide.
    TrackPanel,
}

/// Which half of the pill an image wants.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum FxPart {
    /// The labelled end — the `FX` button.
    #[default]
    Label,
    /// The lit end — the bypass toggle.
    Toggle,
    /// Both halves in one element.
    ///
    /// The split into two images is REAPER's: it blits `mcp_fx` and
    /// `mcp_fxbyp` side by side, so the exporter needs each half on its
    /// own. A live GUI has no such constraint, and composing the halves as
    /// two positioned `<svg>`s put the toggle's rounded end through the
    /// `FX` under Blitz. One element cannot be mis-composed.
    Whole,
}

/// Where the record-arm housing stops flaring and goes vertical.
///
/// Its base corners run 45° out from `0.592` of the cell to here; below it
/// the sides are upright — a plain rectangle. That rectangle is meant to be
/// invisible: REAPER seats it in the dark under the coloured band so only
/// the flare emerges into the colour, which is what makes the housing read
/// as growing out of the background rather than sitting on it. A panel
/// placing the button needs this number to know how far to sink it.
pub const HOUSING_SHOULDER: f32 = 0.717;

/// The measured geometry of one family's FX control.
struct Pill {
    /// Whole pill, both halves.
    w: f32,
    h: f32,
    /// Where the toggle begins.
    split: f32,
    /// Body top and height, as fractions of `h`.
    body: (f32, f32),
    /// Translucent over the strip rather than opaque.
    scrim: bool,
    /// Empty columns between the two halves.
    ///
    /// The mixer leaves one; the track panel leaves none, its halves
    /// abutting directly. Easy to get backwards, because the two families'
    /// cells start at different origins — the mixer's toggle strip at
    /// image column 0 and the track panel's at 1 — so reading both from
    /// column 0 shows the gap in the wrong one.
    gutter: f32,
}

impl FxFamily {
    fn pill(self) -> Pill {
        match self {
            // `mcp_fx_norm` draws a 28-wide cell and `track_fxempty_v` an
            // 18-wide one, rows 1..18 of 22 in both.
            Self::Mixer => Pill {
                w: 46.0,
                h: 22.0,
                split: 28.0,
                body: (1.0 / 22.0, 18.0 / 22.0),
                scrim: false,
                gutter: 1.0,
            },
            // `track_fx_norm` is 20 wide and `track_fxempty_h` 16, rows
            // 1..20 in both.
            Self::TrackPanel => Pill {
                w: 36.0,
                h: 22.0,
                split: 20.0,
                body: (1.0 / 22.0, 20.0 / 22.0),
                scrim: true,
                gutter: 0.0,
            },
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct FxControlProps {
    /// What the chain is doing — drives the label's colour.
    #[props(default)]
    pub chain: FxChain,
    /// What the toggle is showing.
    #[props(default)]
    pub bypass: FxBypass,
    #[props(default)]
    pub family: FxFamily,
    /// Which half to emit.
    #[props(default)]
    pub part: FxPart,
    /// Which band of this half to draw, when the caller is drawing the
    /// pill as a row of bands rather than whole.
    ///
    /// REAPER nine-slices this button: `mcp.fx` is 43 wide and `mcp.fxbyp`
    /// 29, against art that is 28 and 17, and only the flat run before the
    /// seam grows — which is why its `FX` reads small in a long pill.
    /// Scaling the whole thing into those boxes stretches the rounded ends
    /// into a notch and the letters with them.
    ///
    /// The window is relative to the half [`FxControlProps::part`] selects,
    /// so a caller can hand over [`crate::slice::NamedArt::row`]'s panes
    /// unchanged. `None` draws the half whole, which is what the exporter
    /// wants.
    #[props(default)]
    pub pane: Option<crate::slice::Pane>,
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    /// Pointer state — hover lifts the face, pressed sinks it.
    #[props(default)]
    pub at: Interaction,
}

/// The FX control: a labelled button and a lit toggle, as **one pill**.
///
/// REAPER blits these as two images, which is why they were built as two
/// components — and every seam between them then had to be maintained by
/// hand. The toggle's inner corners had to be squared because the button
/// runs flush there; the edge down that side had to be suppressed because
/// there is no edge in the middle of a shape; the two had to be given the
/// same body rows and the same material. Each was found separately, after
/// looking wrong.
///
/// Drawing it as one shape and *windowing* the viewBox for each image
/// makes those relationships structural: the pill is rounded at both outer
/// ends and continuous through the middle because it is one path, and the
/// two images cannot disagree about height or material because neither
/// knows it is an image.
#[component]
pub fn FxControl(props: FxControlProps) -> Element {
    let t = Theme::default();
    let k = ink(None, props.at, true, 0.35);
    let mut p = props.family.pill();

    // Widened by *redrawing*, never by scaling.
    //
    // `mcp.fx` is a 43-wide box and the art behind it is 28; `mcp.fxbyp` is
    // 29 against 18. Asked for those widths with `preserveAspectRatio:
    // none` the whole drawing stretched — 1.54 across and nothing down —
    // which elongated the rounded ends into notches and blew the `FX` up to
    // half again its size. REAPER gets there by nine-slicing: the ends hold
    // their size and only the flat run grows.
    //
    // This is vector art, so it does the same thing the honest way: the
    // slack goes into the pill's own geometry before anything is drawn. The
    // corner radius, the letters and the lamp keep their natural size and
    // the `<svg>` ends up 1:1 with its viewBox.
    let widen = match (props.pane, props.width) {
        (None, Some(want)) => {
            let natural = match props.part {
                FxPart::Label => p.split,
                FxPart::Toggle => p.w - p.split,
                FxPart::Whole => p.w,
            };
            (want as f32 - natural).max(0.0)
        }
        // A caller drawing bands states each band's width itself, and the
        // exporter rasterises at the source size — neither wants slack.
        _ => 0.0,
    };
    // The letters do not move with the widening. REAPER grows the flat run
    // *after* them — its `FX` sits about ten pixels into a 43-wide box, the
    // same place it sits in the 28-wide art — so re-centring the glyph in
    // the widened half pushed it to the middle of the pill, which is the
    // difference that still read as "wrong scaling" once the stretch was
    // gone.
    let glyph_split = p.split;
    match props.part {
        FxPart::Label => {
            p.split += widen;
            p.w += widen;
        }
        FxPart::Toggle => p.w += widen,
        // The label half takes the slack, exactly as it does when the two
        // are drawn separately, so a whole pill and a composed one are the
        // same drawing.
        FxPart::Whole => {
            p.split += widen;
            p.w += widen;
        }
    }

    // The half this image is. The pill is drawn once, at its own
    // proportions, and each image is a window onto it — that is what keeps
    // the two halves from disagreeing about height or material.
    let (part_x, part_w) = match props.part {
        FxPart::Label => (0.0, p.split),
        FxPart::Toggle => (p.split, p.w - p.split),
        FxPart::Whole => (0.0, p.w),
    };
    // A band *within* that half, when the caller is stretching one. Offset
    // into the half rather than into the pill, so a caller can pass the
    // image's own panes without knowing where its half begins.
    let (win_x, win_w) = match props.pane {
        Some(pane) => (part_x + pane.view.0, pane.view.2),
        None => (part_x, part_w),
    };

    let (body_y, body_h) = (p.h * p.body.0, p.h * p.body.1);
    // Exactly one pixel, on the pixel grid.
    //
    // The viewBox is in the source's own pixels, so `1.0` here is one
    // output pixel — and offsetting the path by half of it puts the
    // stroke inside a single column instead of straddling two. At
    // `p.h * 0.05` it was 1.1px centred on an integer, which antialiases
    // across two columns and reads as a soft gradient rather than an
    // edge.
    let edge = 1.0f32;
    let r = p.h * 0.12;

    // One pill, in two pieces with a one-pixel gutter between them.
    //
    // Not a stylistic seam — both families have it. The label half's art
    // ends one column short of the split and the toggle half's begins one
    // past it, leaving that column empty, which is what lets REAPER blit
    // two images side by side and have them read as one shape.
    //
    // Everything that spans the pill has to respect it. A first attempt
    // broke the gutter in the fill only and left the border stroke and
    // the highlight row running straight across, so the column came back
    // filled — from the highlight, two rows down, which looked nothing
    // like the cause.
    let (x, y) = (edge / 2.0, body_y + edge / 2.0);
    let (w, h) = (p.w - edge, body_h - edge);
    let gut = p.gutter;
    // The empty column *is* `split`: the label's art runs up to it and
    // the toggle's begins one past it. Straddling the split instead —
    // half a pixel either side — left both halves' edges mid-pixel and
    // the column half covered.
    let (lhs, rhs) = (p.split, p.split + gut);
    // The two left arcs sweep 0, the two right arcs sweep 1.
    //
    // Not a symmetry to tidy up: with the traversal these paths use, both
    // ends need a different flag to bulge outward, and at sweep 1 the
    // left corners picked the other centre and curved *into* the pill —
    // a notch where the source has a rounded end. The source's own left
    // corner is plainly convex, reaching x2 by row 3 and x1 by row 5.
    let outline = format!(
        "M {} {y} H {lhs} V {} H {} A {r} {r} 0 0 0 {x} {} V {} A {r} {r} 0 0 0 {} {y} Z \
         M {rhs} {y} H {} A {r} {r} 0 0 1 {} {} V {} A {r} {r} 0 0 1 {} {} H {rhs} Z",
        x + r,
        y + h,
        x + r,
        y + h - r,
        y + r,
        x + r,
        x + w - r,
        x + w,
        y + r,
        y + h - r,
        x + w - r,
        y + h,
    );
    // The border, as two *open* paths that stop at the gutter. The halves
    // meet with no edge between them — that is what makes them one pill —
    // so stroking the closed outline drew a line down each side of the
    // seam, half of it landing in the empty column.
    let frame = format!(
        "M {lhs} {y} H {} A {r} {r} 0 0 0 {x} {} V {} A {r} {r} 0 0 0 {} {} H {lhs} \
         M {rhs} {y} H {} A {r} {r} 0 0 1 {} {} V {} A {r} {r} 0 0 1 {} {} H {rhs}",
        x + r,
        y + r,
        y + h - r,
        x + r,
        y + h,
        x + w - r,
        x + w,
        y + r,
        y + h - r,
        x + w - r,
        y + h,
    );

    // Bypassing darkens the plate as well as reddening the letters: the
    // label half runs 77 to 57 normally and 67 to 49 bypassed, a flat
    // 0.87 on both ends. Changing only the text left the button reading
    // as active with odd lettering.
    let dull = if props.chain == FxChain::Bypassed {
        0.87f32
    } else {
        1.0
    };
    let plate_top = lift(k.face.shade(0.07), dull - 1.0).css();
    let plate_bot = lift(k.face.shade(-0.10), dull - 1.0).css();

    // The plate varies with state and the outline with family and widen —
    // a hovered pill next to a resting one, or a track-panel pill in the
    // same document as a mixer's, must not share defs.
    let face_id = vary("fxface", &[&plate_top, &plate_bot]);
    // `split` as well as the box: widen moves it for Label and Whole but
    // not Toggle, so two parts can share (w, h, widen) with different
    // outlines.
    let clip_id = vary(
        "fxpill",
        &[&format!("{}x{}x{}x{}", p.w, p.h, widen, p.split)],
    );
    let (fill, alpha) = if p.scrim {
        ("#000000".to_string(), 0.35)
    } else {
        (format!("url(#{face_id})"), 1.0)
    };

    // Neutral, like everything else printed on a hardware control. The
    // source letters are #9c9c9c empty, #dadada active and a desaturated
    // #c34a54 bypassed — `text_faint` and `text` are the chrome ramp's
    // blues and read as lit indicators rather than print on plastic.
    // The two families print their letters differently, and it is not one
    // being a shade of the other: the mixer's are dimmer and fully
    // opaque, the track panel's brighter but semi-transparent, because
    // they sit on a scrim rather than on plastic.
    //
    //     empty   mixer #9c at 1.00     track #c1 at 0.61
    //     active  mixer #db at 1.00     track #eb at 0.87
    let (text, text_alpha) = match (props.chain, p.scrim) {
        (FxChain::Empty, false) => (t.chrome.hardware_mark.shade(-0.06), 1.0f32),
        (FxChain::Empty, true) => (t.chrome.hardware_mark.shade(0.33), 0.61),
        (FxChain::Active, false) => (t.chrome.hardware_mark.shade(0.61), 1.0),
        (FxChain::Active, true) => (t.chrome.hardware_mark.shade(0.78), 0.87),
        (FxChain::Bypassed, _) => (t.signal.mute, 1.0),
    };
    // The letters light with the button. Measured per cell:
    //
    //     mcp_fx_norm      219 / 233 / 219
    //     track_fx_norm    235 / 242 / 221
    //
    // so hover lifts about a third of the headroom in both, and pressed
    // returns to rest in the mixer but sits *below* it in the track
    // panel. Drawn flat, the hover cell was the one the eye went to and
    // the one that had not changed.
    let text = match props.at {
        Interaction::Normal => text,
        Interaction::Hover => text.shade(0.35),
        Interaction::Pressed if p.scrim => text.shade(-0.08),
        Interaction::Pressed => text,
    };

    // The toggle's lamp, in pill coordinates.
    let lamp = match props.bypass {
        FxBypass::Empty => None,
        FxBypass::On => Some(t.chrome.accent),
        FxBypass::Off => Some(t.signal.meter_danger),
    };
    let plus = props.bypass == FxBypass::Empty && props.at != Interaction::Normal;
    // 8px from the toggle's left edge, not centred in it. The lens sits
    // at the same offset in both families — which *is* centred in the
    // 16-wide half and a pixel left of centre in the 18-wide one, so
    // centring it put the mixer's a pixel too far right.
    // 0.364 of the height from the split, which is the middle of the
    // toggle's natural 18 — so a widened toggle carries the lamp along with
    // half the slack rather than leaving it against the seam.
    let (tx, ty) = (
        p.split
            + p.h * 0.364
            + if props.part == FxPart::Toggle {
                widen * 0.5
            } else {
                0.0
            },
        body_y + body_h * 0.5,
    );
    // Measured: a 4px pill and an 8x8 plus with 2px arms, in both
    // families — so neither scales with the cell width.
    let (pw, ph) = (p.h * 0.182, body_h * 0.5);
    let (arm, bar) = (p.h * 0.364, p.h * 0.091);

    rsx! {
        svg {
            // A stretched band must not letterbox: the flat run is meant to
            // lengthen, which is the whole reason it is its own band.
            preserve_aspect_ratio: "none",
            width: "{props.width.unwrap_or(win_w as u32)}",
            height: "{props.height.unwrap_or(p.h as u32)}",
            // And again in CSS. An `<svg>` is a replaced element and Blitz
            // sizes it from its box before it looks at the attributes, so
            // with an auto-width box the two halves divided the pill
            // between them: the label came out a stub showing only its
            // rounded end and the toggle stretched across the rest.
            style: "display:block; width:{props.width.unwrap_or(win_w as u32)}px; \
                    height:{props.height.unwrap_or(p.h as u32)}px;",
            // The window is the only thing that differs between the two
            // images: same drawing, different slice of it.
            view_box: "{win_x} 0 {win_w} {p.h}",
            xmlns: "http://www.w3.org/2000/svg",
            defs {
                linearGradient { id: "{face_id}", x1: "0", y1: "0", x2: "0", y2: "1",
                    stop { offset: "0", stop_color: "{plate_top}" }
                    stop { offset: "1", stop_color: "{plate_bot}" }
                }
                clipPath { id: "{clip_id}",
                    path { d: "{outline}" }
                }
            }
            // The seam is only truly empty at rest. Under the pointer and
            // pressed, the source washes it over at about a third alpha —
            // 97 of 255 bypassed, 59 lit — so the two halves stop reading
            // as two buttons the moment you touch them.
            if gut > 0.0 && props.at != Interaction::Normal {
                rect {
                    x: "{lhs}", y: "{body_y + edge}",
                    width: "{gut}", height: "{body_h - edge * 2.0}",
                    fill: "{k.border.css()}", fill_opacity: "0.30",
                }
            }
            path { d: "{outline}", fill: "{fill}", fill_opacity: "{alpha}" }
            // The toggle end is recessed. Reading the two halves down
            // their centres gives 77 to 57 on the label and 64 to 49 on
            // the toggle — the same gradient with a constant sixth taken
            // off it, not a second gradient. One plate for both put the
            // toggle 39 levels light.
            rect {
                x: "{rhs}", y: "{body_y}",
                width: "{p.w - rhs}", height: "{body_h}",
                fill: "#000000", fill_opacity: "0.16",
                clip_path: "url(#{clip_id})",
            }
            path {
                d: "{frame}",
                fill: "none",
                stroke: "{k.border.css()}",
                stroke_width: "{edge}",
            }
            // The lit row just inside the top, which every ReaperTips
            // control has — one per half, stopping at the gutter.
            rect {
                x: "{x + r}", y: "{y + edge}",
                width: "{lhs - x - r}", height: "{p.h * 0.045}",
                fill: "#ffffff", fill_opacity: "0.07",
            }
            rect {
                x: "{rhs}", y: "{y + edge}",
                width: "{x + w - r - rhs}", height: "{p.h * 0.045}",
                fill: "#ffffff", fill_opacity: "0.07",
            }

            text {
                // Centred on the label half, plus two.
                //
                // `text-anchor: middle` centres the string's *advance*,
                // which includes the trailing letter-space and both side
                // bearings — none of which the source's hand-set letters
                // carry. Measured against the F's stem, a plain centre
                // lands the pair two columns left of where the art puts
                // them in both families.
                x: "{glyph_split * 0.5 + 1.5}", y: "{body_y + body_h * 0.5}",
                text_anchor: "middle", dominant_baseline: "central",
                font_family: "Fira Sans, DejaVu Sans, sans-serif",
                // 0.51, not 0.44. The peak colour is right either way —
                // what differs is *coverage*: at 0.44 the source has twice
                // as many bright pixels as ours, so the letters read dim
                // rather than dark. Sized so the caps come out 7px in a
                // 22px cell, which the glyph-row test pins.
                font_weight: "700",
                font_size: "{p.h * 0.51}",
                letter_spacing: "{p.h * 0.03}",
                fill: "{text.css()}",
                fill_opacity: "{text_alpha}",
                "FX"
            }

            if plus {
                rect {
                    x: "{tx - arm * 0.5}", y: "{ty - bar * 0.5}",
                    width: "{arm}", height: "{bar}",
                    fill: "{t.chrome.hardware_mark.shade(0.33).css()}",
                }
                rect {
                    x: "{tx - bar * 0.5}", y: "{ty - arm * 0.5}",
                    width: "{bar}", height: "{arm}",
                    fill: "{t.chrome.hardware_mark.shade(0.33).css()}",
                }
            } else if let Some(c) = lamp {
                // The halo is what separates an LED from a coloured slab.
                rect {
                    x: "{tx - pw * 0.85}", y: "{ty - ph * 0.62}",
                    width: "{pw * 1.7}", height: "{ph * 1.24}",
                    rx: "{pw * 0.85}",
                    fill: "{c.css()}", fill_opacity: "0.22",
                }
                rect {
                    x: "{tx - pw * 0.5}", y: "{ty - ph * 0.5}",
                    width: "{pw}", height: "{ph}",
                    rx: "{pw * 0.5}",
                    fill: "{c.css()}",
                }
            } else {
                // Dormant: a *recessed* lens. Light in the middle —
                // #656565, brighter than the face — inside a ring that is
                // darker than the face again. Measured across the source
                // it goes 65, 58, 102, 102, 58, 65: without the ring it
                // reads as a flat light blob stuck on rather than a lamp
                // set into the plastic.
                // Ring behind, core on top — not a stroke, which
                // straddles the edge it is drawn on and ate half a pixel
                // off each side, leaving a 3px core where the source has
                // 4 (cols 6..9) inside a ring (cols 5 and 10).
                //
                // Only on the opaque family: against a scrim the source
                // puts the lens straight onto the black, with nothing
                // between.
                if !p.scrim {
                    rect {
                        x: "{tx - pw * 0.5 - 1.0}", y: "{ty - ph * 0.5 - 1.0}",
                        width: "{pw + 2.0}", height: "{ph + 2.0}",
                        rx: "{pw * 0.5 + 1.0}",
                        fill: "{t.chrome.hardware.shade(-0.1).css()}",
                    }
                }
                rect {
                    x: "{tx - pw * 0.5}", y: "{ty - ph * 0.5}",
                    width: "{pw}", height: "{ph}",
                    rx: "{pw * 0.5}",
                    fill: "{t.chrome.hardware_mark.shade(-0.38).css()}",
                }
            }
        }
    }
}

// ── record arm: a ring ───────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
pub struct RecordArmProps {
    #[props(default)]
    pub state: RecordArm,
    /// The REAPER image this draws: the box its art is drawn at, and which
    /// band of that box takes up slack when it is drawn bigger — see
    /// [`NamedArt`]. Deliberately without a default, because one component
    /// serves several images at several sizes.
    pub art: NamedArt,
    /// Draw the moulded housing the ring is seated in.
    ///
    /// The mixer has one; the track panel draws a bare ring on the strip.
    /// Keeping it a prop rather than inferring it from the cell size means
    /// a narrow mixer cell still gets its housing.
    #[props(default = true)]
    pub housing: bool,
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    /// Pointer state — hover lifts the face, pressed sinks it.
    #[props(default)]
    pub at: Interaction,
}

#[component]
pub fn RecordArmButton(props: RecordArmProps) -> Element {
    let t = Theme::default();
    let armed = matches!(
        props.state,
        RecordArm::On | RecordArm::NoRecord | RecordArm::AutoOn | RecordArm::AutoNoRecord
    );
    let auto = matches!(
        props.state,
        RecordArm::Auto | RecordArm::AutoOn | RecordArm::AutoNoRecord
    );
    let barred = matches!(props.state, RecordArm::NoRecord | RecordArm::AutoNoRecord);

    let (vw, vh) = props.art.source;
    let unit = vw.min(vh);

    // Traced, in *edge* coordinates rather than pixel indices. In the
    // mixer's 36x24 the ring covers columns 10..24 — the span [10, 25) —
    // so it is centred on 17.5 with radius 7.5, and rows 5..19, centred on
    // 12.5. Reading the indices directly gives 17 and 12 and puts the whole
    // control half a pixel up and to the left.
    //
    // The track panel's 20x20 ring is centred and **larger** relative to
    // its cell (radius 8 of 20) because there is no housing competing for
    // the room, so the two fractions differ rather than one being wrong.
    // **The barred ring is not the plain ring with an X over it.** It is
    // a bigger, thinner band in both families — the mixer's runs 8.46
    // outside and 4.56 in against 7.45 and 3.67, the track panel's 7.91
    // and 4.28 against 7.40 and 3.38. Sharing one pair of radii left the
    // outer edge sixty to eighty levels short all the way round, which
    // made `track_recarm_norec` the worst image in the set after the
    // thumbs.
    // Only the *plain* barred states grow. `AutoNoRecord` keeps the
    // ordinary radii — measured 7.40 and 3.35 in the track panel, the
    // same as `off` — so it is the auto disc that governs there, not the
    // bar. Applying the enlargement to it put that image up from under
    // ten to fifteen.
    let wide = barred && !auto;
    let (cx, cy, outer, hole) = if props.housing && wide {
        (vw * 0.486, vh * 0.521, unit * 0.3524, unit * 0.1899)
    } else if props.housing {
        (vw * 0.486, vh * 0.521, unit * 0.3105, unit * 0.1530)
    } else if wide {
        (vw * 0.5, vh * 0.5, unit * 0.3953, unit * 0.2139)
    } else {
        // Read off the coverage rather than guessed at from a threshold.
        // Down the widest row the alpha runs 103, 255 ... 255, 102: the
        // outer edge is 40% into the pixel at x=2, so it stands at 2.60
        // and the radius is 7.40, not the 8 that a "first lit column"
        // reading gives. Likewise the hole's rim reads 96 at x=6, putting
        // its boundary at 6.62 and its radius at 3.38.
        //
        // Both variants agree on both numbers, which is the check that
        // they are the shape and not the threshold.
        (vw * 0.5, vh * 0.5, unit * 0.370, unit * 0.169)
    };
    // The ring is an annulus *path*, not a stroked circle.
    //
    // A stroke took the gradient as a flat average — the whole ring came
    // out one value where the source runs 239 to 251 — and it also
    // straddles its own radius, so the crisp outer edge had to be found
    // by nudging. A fill with an even-odd hole gradates properly and puts
    // its boundary exactly where the number says.
    let annulus = format!(
        "M {} {cy} A {outer} {outer} 0 1 0 {} {cy} A {outer} {outer} 0 1 0 {} {cy} Z \
         M {} {cy} A {hole} {hole} 0 1 1 {} {cy} A {hole} {hole} 0 1 1 {} {cy} Z",
        cx - outer,
        cx + outer,
        cx - outer,
        cx - hole,
        cx + hole,
        cx - hole,
    );

    // Neutral when unarmed — the source ring is #a6a6a6, which is
    // `hardware_mark`. It was `text_dim`, a blue-grey that made a disarmed
    // track look faintly lit.
    let ring = if armed {
        t.signal.rec
    } else {
        t.chrome.hardware_mark
    };
    // The two families ramp completely differently, and the source is the
    // authority on both. The mixer's housing lifts a little and sinks a
    // little. The track panel's bare ring does not sink at all — its
    // pressed cell is the same #a6a6a6 as its normal one — and its hover
    // goes all the way to #d9d9d9, more than half the way to white, where
    // the old shared 0.15 lift was invisible at this size. Its `auto` disc
    // is gentler again (#bfbfbf) and its armed reds gentler still.
    let ring = match props.at {
        Interaction::Normal => ring,
        Interaction::Hover if props.housing => ring.shade(0.15),
        // Barred hovers far harder than merely armed: #f44b4e goes to
        // #fc9b9c, most of the way to white, where a plain armed ring
        // moves #f94d5d to #fe5b5c and barely shifts. Sharing one amount
        // left the barred button's hover invisible.
        // And it mixes toward white rather than scaling: a scale clips
        // red at the ceiling and leaves green and blue far behind, where
        // the source's #fc9b9c has all three well up.
        Interaction::Hover if barred => ring.shade(0.44),
        Interaction::Hover if armed => ring.shade(0.06),
        Interaction::Hover if auto => ring.shade(0.29),
        Interaction::Hover => ring.shade(0.57),
        Interaction::Pressed if props.housing => ring.shade(-0.12),
        Interaction::Pressed => ring,
    };
    // Unarmed and unhoused, the source ring is a single flat colour — 104
    // pixels of exactly #a6a6a6, no gradient anywhere. Only the armed reds
    // and the moulded mixer ring are lit from above.
    let flat = !props.housing && !armed;
    // The hole is not a window onto the surface behind — it is the housing
    // showing through, and in the source both are the same #262626. Filling
    // it from `surface` punched a blue-black hole through the middle.
    //
    // Without a housing there is nothing behind it, so the hole is a hole.
    // The ring's lit end, and **not** `shade`: that mixes toward white,
    // and #e23b53 is already near the top of its red channel, so
    // `shade(0.14)` moved it by four points and the gradient rendered
    // flat. The source's highlight is a different red — #fa4e5e — which
    // is most of the way from `rec` to `meter_danger`, both of which are
    // measured from this theme already.
    let ring_hi = if armed {
        ring.mix(t.signal.meter_danger, 0.8)
    } else {
        ring.shade(0.18)
    };
    let hole_fill = t.chrome.hardware.shade(-0.40);
    // The ring's colours vary with state and the masks' geometry with the
    // cell, so all three ids are derived from what they draw.
    let ring_id = vary("recring", &[&ring_hi.css(), &ring.css()]);
    let ring_paint = if flat {
        ring.css()
    } else {
        format!("url(#{ring_id})")
    };
    let auto_id = vary("recauto", &[&format!("{vw}x{vh}")]);
    // With a housing, the hole is that housing showing through — #262626
    // in the source, the same colour as the surround, so it is painted
    // *behind* the ring. Without one there is nothing behind: the
    // source's hole is fully transparent, and filling it laid a grey disc
    // over the strip that read as a much thicker ring.
    //
    // Cuts then need something to cut *with*, and over nothing that has
    // to be a mask, since "paint transparent" does not erase.
    let notch_id = vary("recnotch", &[&format!("{vw}x{vh}")]);

    rsx! {
        svg {
            width: "{props.width.unwrap_or(vw as u32)}",
            height: "{props.height.unwrap_or(vh as u32)}",
            view_box: "0 0 {vw} {vh}",
            xmlns: "http://www.w3.org/2000/svg",
            defs {
                // The ring is lit from the top, like every other moulded
                // part here: the source carries #fa4e5e and #e23b53 in
                // equal measure and this drew only the darker of the two,
                // which is why it read flat and dull beside the original.
                linearGradient { id: "{ring_id}", x1: "0", y1: "0", x2: "0", y2: "1",
                    stop { offset: "0", stop_color: "{ring_hi.css()}" }
                    stop { offset: "1", stop_color: "{ring.css()}" }
                }
            }
            if props.housing {
                // Traced off `mcp_recarm_on` by **sub-pixel coverage**, not
                // by thresholding the alpha. Down column 6 the alpha runs
                // 49, 101, 153, 205, 244 from y=8 to y=12 — an edge still
                // creeping outward at about a fifth of a pixel per row,
                // where a threshold reports a hard vertical line at x=6 for
                // six rows running. Every earlier reading of this shape was
                // built on that phantom straight edge.
                //
                // What it is: a circle of radius 11.5 centred on
                // (17.5, 12.3) — concentric with the ring, give or take —
                // that goes flat near its widest point, sitting on a base
                // whose top corners are 45° flares. No vertical section.
                circle {
                    // 12.5, not 12.3: at 12.3 the circle's top lands at
                    // y=0.8 and catches row 0, where the source starts at
                    // row 1. Half a pixel of centre, a whole row of art.
                    cx: "{cx}", cy: "{vh * 0.5208}", r: "{vw * 0.3194}",
                    fill: "{hole_fill.css()}",
                }
                path {
                    d: "M {cx - vw * 0.3194} {vh * 0.592}
                        L {cx - vw * 0.4028} {vh * HOUSING_SHOULDER} V {vh}
                        H {cx + vw * 0.4028} V {vh * HOUSING_SHOULDER}
                        L {cx + vw * 0.3194} {vh * 0.592} Z",
                    fill: "{hole_fill.css()}",
                }
            }
            // Auto is a **solid disc with the A knocked out of it**, not a
            // ring with a letter laid inside. Drawing both gave a grey
            // annulus with a second grey A floating in its hole — two
            // marks where the source has one.
            if auto {
                if props.housing {
                    circle {
                        cx: "{cx}", cy: "{cy}", r: "{outer}",
                        fill: "{ring_paint}",
                    }
                    text {
                        x: "{cx}", y: "{cy}",
                        text_anchor: "middle", dominant_baseline: "central",
                        font_family: "Fira Sans, DejaVu Sans, sans-serif",
                        font_weight: "700", font_size: "{outer * 1.6}",
                        // The housing colour, so the glyph is a hole through
                        // the disc rather than ink on top of it.
                        fill: "{hole_fill.css()}",
                        "A"
                    }
                } else {
                    // On the strip there is no housing to show through, so
                    // the A is a *hole*: fully transparent in the source,
                    // same as the ring's centre. Painting it any colour
                    // laid a grey letter on the strip instead of cutting
                    // one out of the disc, and "paint transparent" does not
                    // erase — it has to be a mask.
                    // Drawn, not set. The source's A is far wider for its
                    // height than any A the font gives — 8 pixels across a
                    // base 8 tall, with a flat apex two pixels wide — and
                    // a path does not depend on a font being installed,
                    // which for art baked at build time matters.
                    //
                    // Traced by half-width per row: the outer edge runs
                    // 1.9 at the top to 4.25 at the base, and the counter
                    // opens at y10.7 and reaches 2.15, which leaves the
                    // two-pixel legs the source has.
                    defs {
                        mask { id: "{auto_id}",
                            rect {
                                x: "0", y: "0", width: "{vw}", height: "{vh}",
                                fill: "#ffffff",
                            }
                            path {
                                d: "M {cx - outer * 0.257} {cy - outer * 0.730}
                                    H {cx + outer * 0.257}
                                    L {cx + outer * 0.574} {cy + outer * 0.419}
                                    H {cx - outer * 0.574} Z",
                                fill: "#000000",
                            }
                            path {
                                d: "M {cx} {cy + outer * 0.095}
                                    L {cx + outer * 0.291} {cy + outer * 0.419}
                                    H {cx - outer * 0.291} Z",
                                fill: "#ffffff",
                            }
                        }
                    }
                    circle {
                        cx: "{cx}", cy: "{cy}", r: "{outer}",
                        fill: "{ring_paint}",
                        mask: "url(#{auto_id})",
                    }
                }
            } else {
                // Four radial notches when barred, not two crossing lines:
                // the original reads as a life-ring, with the cuts running
                // through the band to the outer edge. They are a mask so
                // they cut the ring whether or not anything is behind it.
                if props.housing {
                    circle {
                        cx: "{cx}", cy: "{cy}", r: "{hole}",
                        fill: "{hole_fill.css()}",
                    }
                }
                if barred {
                    defs {
                        mask { id: "{notch_id}",
                            rect {
                                x: "0", y: "0", width: "{vw}", height: "{vh}",
                                fill: "#ffffff",
                            }
                            // Two straight bars, crossing at the
                            // centre — an **X laid over the whole ring**,
                            // not four wedges cut out of it.
                            //
                            // Both leave four blobs at this size, which is
                            // why wedges looked plausible, but they are
                            // not the same shape: a wedge's cut edges
                            // point at the centre, and the source's are
                            // parallel. Mapping the pixels shows the top
                            // blob narrowing from 8 columns to 6 over two
                            // rows, which is a straight edge crossing it,
                            // not a radial one.
                            for (i, deg) in [45.0f32, 135.0].iter().enumerate() {
                                {
                                    let a = deg.to_radians();
                                    let (dx, dy) = (a.cos(), a.sin());
                                    let far = outer + unit * 0.1;
                                    rsx! {
                                        line {
                                            key: "{i}",
                                            x1: "{cx - dx * far}", y1: "{cy - dy * far}",
                                            x2: "{cx + dx * far}", y2: "{cy + dy * far}",
                                            stroke: "#000000",
                                            // Scaled off the ring, not the
                                            // cell. As a fraction of the
                                            // cell the two families pull
                                            // in opposite directions —
                                            // what suits the mixer's 24
                                            // over-cuts the track panel's
                                            // 20 — because the notch
                                            // tracks the band it crosses.
                                            stroke_width: "{outer * 0.29}",
                                        }
                                    }
                                }
                            }
                        }
                    }
                    path {
                        d: "{annulus}",
                        fill: "{ring_paint}",
                        fill_rule: "evenodd",
                        mask: "url(#{notch_id})",
                    }
                } else {
                    path {
                        d: "{annulus}",
                        fill: "{ring_paint}",
                        fill_rule: "evenodd",
                    }
                }
            }
        }
    }
}

// ── routing: stacked bars ────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
pub struct RoutingProps {
    #[props(default)]
    pub has_sends: bool,
    #[props(default)]
    pub has_receives: bool,
    #[props(default)]
    pub disabled: bool,
    /// The REAPER image this draws: the box its art is drawn at, and which
    /// band of that box takes up slack when it is drawn bigger — see
    /// [`NamedArt`]. Deliberately without a default, because one component
    /// serves several images at several sizes.
    pub art: NamedArt,
    /// Mixer stacks the lanes; the track panel sets them in a row.
    #[props(default)]
    pub axis: Axis,
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    /// Pointer state — hover lifts the face, pressed sinks it.
    #[props(default)]
    pub at: Interaction,
}

#[component]
pub fn RoutingButton(props: RoutingProps) -> Element {
    let t = Theme::default();
    let k = ink(None, props.at, true, 0.35);
    let (vw, vh) = props.art.source;

    // Three lanes, not two: the original stacks the track's own output,
    // its sends and its receives. Two bars cannot express what the
    // control reports.
    //
    // Read off the source cells: the top lane is blue in all of them — a
    // track always has an output — and only the lower two light up.
    // Colouring it conditionally made an unrouted track look broken
    // rather than merely unrouted.
    //
    // An unlit lane is #6c6c6c — plain grey. It was `text_faint`, a
    // blue-grey, so a track with nothing routed looked faintly lit.
    // An unlit lane is grey in both families, but only opaque in one:
    // the mixer's is a solid #6c6c6c on plastic, the track panel's a
    // #727272 at half alpha over the strip. Drawing both solid left the
    // track panel's unrouted lanes reading as lit.
    let dim = if props.axis == Axis::Horizontal {
        (t.chrome.hardware_mark.shade(-0.29), 0.49f32)
    } else {
        (t.chrome.hardware_mark.shade(-0.33), 1.0)
    };
    // Disabled greys the *output* lane and nothing else.
    //
    // Compared cell for cell, `mcp_io_s_r` and `mcp_io_s_r_dis` differ in
    // exactly one place: the top lane goes from #5dc1fe to #6e6e6e. The
    // plate, the amber send, the red receive and every alpha are
    // identical. This drew the whole button at 40% instead, which dimmed
    // things the source leaves alone and left the one thing it does dim
    // fully blue.
    let out = if props.disabled {
        dim
    } else {
        (t.chrome.accent, 1.0)
    };
    let send = if props.has_sends {
        (t.signal.meter_warn, 1.0)
    } else {
        dim
    };
    // `meter_danger`, not `rec`: the source uses a brighter #ff5260 for a
    // lit lane than the #e23b53 of the record ring.
    let recv = if props.has_receives {
        (t.signal.meter_danger, 1.0)
    } else {
        dim
    };

    // Traced lane geometry, per family. The mixer runs three 11x4 bars
    // down a 23x32 cell at y=6, 13, 20; the track panel runs three 4x10
    // bars across a 28x22 cell at x=5, 12, 19. Same rhythm — pitch 7 —
    // turned a quarter turn, but *not* the same fractions: guessing them
    // as "56% of the width, centred" put every lane a pixel and a bit
    // left of the art.
    let horizontal = props.axis == Axis::Horizontal;
    // x, y, w, h of the panel as fractions of the cell — traced.
    let (box_x, box_y, box_w, box_h) = if horizontal {
        (0.0, 1.0 / 22.0, 1.0, 20.0 / 22.0)
    } else {
        (1.0 / 23.0, 1.0 / 32.0, 21.0 / 23.0, 28.0 / 32.0)
    };
    let (lane_l, lane_t) = if horizontal {
        (vw * 5.0 / 28.0, vw * 7.0 / 28.0)
    } else {
        (vh * 6.0 / 32.0, vh * 7.0 / 32.0)
    };
    let (bar_w, bar_h) = if horizontal {
        (vw * 4.0 / 28.0, vh * 10.0 / 22.0)
    } else {
        (vw * 11.0 / 23.0, vh * 4.0 / 32.0)
    };
    let cross = if horizontal {
        (vh - bar_h) / 2.0
    } else {
        (vw - bar_w) / 2.0
    };
    let r = bar_w.min(bar_h) / 2.0;
    let edge = vh * 0.03;
    let (panel, panel_alpha) = if horizontal {
        ("#000000".to_string(), 0.35)
    } else {
        // #464646 measured, which is 0.04 above the hardware grey — not
        // the 0.10 this had, which read #565656 and made the mixer's
        // panel noticeably paler than the track panel's beside it.
        (k.face.shade(0.04).css(), 1.0)
    };

    rsx! {
        svg {
            width: "{props.width.unwrap_or(vw as u32)}",
            height: "{props.height.unwrap_or(vh as u32)}",
            view_box: "0 0 {vw} {vh}",
            xmlns: "http://www.w3.org/2000/svg",
            // The panel does not fill its cell. Measured: the mixer's is
            // 21x28 in a 23x32 cell, the track panel's 28x20 in 28x22 —
            // so the inset differs by axis rather than being one margin.
            // Filling the cell made both buttons visibly chunkier than
            // the art beside them.
            // Inset by half the stroke, which straddles the edge it is
            // drawn on: without this the button measures two pixels wider
            // and taller than the art it replaces, in both families at
            // once — which reads as "the sizing is wrong" rather than as
            // a stroke problem.
            rect {
                x: "{vw * box_x + edge / 2.0}", y: "{vh * box_y + edge / 2.0}",
                width: "{vw * box_w - edge}", height: "{vh * box_h - edge}",
                rx: "{vw.min(vh) * 0.16}",
                // The two families are not the same fill at two
                // brightnesses. The mixer's panel is an opaque #464646 —
                // a touch lighter than plain hardware grey. The track
                // panel's is **black at 35% alpha**, a scrim that lets the
                // track colour through, which is why it looks near-black
                // on a dark strip and tinted on a coloured one. Painting
                // both opaque made the track buttons sit on the strip
                // instead of in it.
                fill: "{panel}",
                fill_opacity: "{panel_alpha}",
                stroke: "{k.border.css()}", stroke_width: "{edge}",
            }
            rect {
                x: "{vw * box_x + vw * 0.08}", y: "{vh * box_y + edge}",
                width: "{vw * box_w - vw * 0.16}", height: "{vh * 0.04}",
                fill: "#ffffff", fill_opacity: "0.07",
            }
            for (i, lane) in [out, send, recv].iter().enumerate() {
                rect {
                    key: "{i}",
                    x: if horizontal { "{lane_l + lane_t * i as f32}" } else { "{cross}" },
                    y: if horizontal { "{cross}" } else { "{lane_l + lane_t * i as f32}" },
                    width: "{bar_w}", height: "{bar_h}",
                    rx: "{r}",
                    fill: "{lane.0.css()}",
                    fill_opacity: "{lane.1}",
                }
            }
        }
    }
}

// ── input monitoring: concentric arcs ────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
pub struct MonitoringProps {
    #[props(default)]
    pub state: Monitoring,
    /// The REAPER image this draws: the box its art is drawn at, and which
    /// band of that box takes up slack when it is drawn bigger — see
    /// [`NamedArt`]. Deliberately without a default, because one component
    /// serves several images at several sizes.
    pub art: NamedArt,
    /// Mixer radiates downward; the track panel radiates right.
    #[props(default)]
    pub axis: Axis,
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    /// Pointer state — hover lifts the face, pressed sinks it.
    #[props(default)]
    pub at: Interaction,
}

/// The input-monitor icon — a source with waves coming off it.
///
/// The two families are different drawings, not one drawing at two sizes,
/// so every number here is measured per family. Radial alpha profiles,
/// taken in a cone about the direction of travel:
///
/// ```text
/// mixer  (21x20)  origin (10.9,  4.6)  dot 1.5  rings 4.85, 9.15  stroke 2.2
/// track  (15x24)  origin ( 3.5, 12.0)  dot 1.35 rings 4.25, 8.30  stroke 1.95
///
/// Both span the same arc. The outer ring's angular profile is flat from
/// -47 to +47 degrees and gone by 50 — but that is the *painted* extent,
/// which a round cap widens by the cap radius over the ring radius, about
/// seven degrees here. The path itself runs to +-40.
/// ```
///
/// The mixer icon also carries a soft black halo, which is most of what
/// gives it weight — the earlier version had none and read as a flat
/// stencil. The track icon has none: it is a thin line drawing.
///
/// `off` is not the icon dimmed. It is a *dim* icon under a *bright*
/// slash, the slash being the lit element — #5a under #a6 in the mixer,
/// #4c under #c0 in the track panel — which is why the previous version's
/// same-colour slash-on-icon read as a scribble.
#[component]
pub fn InputMonitorIndicator(props: MonitoringProps) -> Element {
    let t = Theme::default();
    let (vw, vh) = props.art.source;
    let mark = t.chrome.hardware_mark;

    // Mixer radiates downward from a dot near the top; the track panel
    // radiates rightward from a dot near the left.
    let horizontal = props.axis == Axis::Horizontal;
    let (cx, cy, deg) = if horizontal {
        (3.5f32, 12.0f32, 0.0f32)
    } else {
        (11.0f32, 4.6f32, 90.0f32)
    };
    let (dot, rings, sw, half) = if horizontal {
        (1.35f32, [4.25f32, 8.3], 1.95f32, 40.0f32)
    } else {
        (1.5f32, [4.85f32, 9.15], 2.2f32, 40.0f32)
    };

    // Hover brightens hard — the mixer's lit icon goes #a6 to #e6, half
    // again as bright. The old ramp lifted it by 0.18, which at this size
    // is invisible.
    let lift = match props.at {
        Interaction::Hover => {
            if horizontal {
                0.28
            } else {
                0.65
            }
        }
        _ => 0.0,
    };
    let dim = if horizontal { -0.53 } else { -0.44 };
    let (ink, slash) = match props.state {
        Monitoring::On => (mark.shade(lift), None),
        // #ff5260 exactly — `signal.rec` is a different red.
        Monitoring::Auto => (t.signal.meter_danger.shade(lift * 0.45), None),
        Monitoring::Off => (
            mark.shade(dim + lift * 0.8),
            Some(mark.shade(if horizontal { 0.20 } else { lift })),
        ),
    };

    let arc = |r: f32| {
        let (a, b) = ((deg - half).to_radians(), (deg + half).to_radians());
        format!(
            "M {} {} A {r} {r} 0 0 1 {} {}",
            cx + a.cos() * r,
            cy + a.sin() * r,
            cx + b.cos() * r,
            cy + b.sin() * r,
        )
    };
    // Bottom-left to top-right, traced off the lit pixels in the `off`
    // cells. The endpoints are pulled in by the cap radius so the round
    // caps land where the source's ends do rather than half a pixel past.
    let (sx0, sy0, sx1, sy1) = if horizontal {
        (3.6f32, 18.8f32, 11.9f32, 4.5f32)
    } else {
        (3.9f32, 16.6f32, 17.2f32, 2.9f32)
    };
    let slash_w = if horizontal { 1.45f32 } else { 1.7 };

    rsx! {
        svg {
            width: "{props.width.unwrap_or(vw as u32)}",
            height: "{props.height.unwrap_or(vh as u32)}",
            view_box: "0 0 {vw} {vh}",
            xmlns: "http://www.w3.org/2000/svg",
            // The halo, in two passes: the source's falls off over about
            // three pixels, and one flat ring reads as an outline instead.
            if !horizontal {
                for (i, spread) in [(3.0f32, 0.15f32), (1.5, 0.24)].iter().enumerate() {
                    g { key: "halo{i}", opacity: "{spread.1}",
                        circle { cx: "{cx}", cy: "{cy}", r: "{dot + spread.0 * 0.5}", fill: "#000000" }
                        for (j, r) in rings.iter().enumerate() {
                            path {
                                key: "{j}",
                                d: "{arc(*r)}",
                                fill: "none",
                                stroke: "#000000",
                                stroke_width: "{sw + spread.0}",
                                stroke_linecap: "round",
                            }
                        }
                    }
                }
            }
            circle { cx: "{cx}", cy: "{cy}", r: "{dot}", fill: "{ink.css()}" }
            for (i, r) in rings.iter().enumerate() {
                path {
                    key: "{i}",
                    d: "{arc(*r)}",
                    fill: "none",
                    stroke: "{ink.css()}",
                    stroke_width: "{sw}",
                    stroke_linecap: "round",
                }
            }
            if let Some(slash) = slash {
                line {
                    x1: "{sx0}", y1: "{sy0}", x2: "{sx1}", y2: "{sy1}",
                    stroke: "{slash.css()}",
                    stroke_width: "{slash_w}",
                    stroke_linecap: "round",
                }
            }
        }
    }
}

// ── pan: a knob with a pointer ───────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
pub struct PanProps {
    /// -1 hard left, 0 centre, +1 hard right.
    #[props(default = 0.0)]
    pub position: f32,
    #[props(default)]
    pub large: bool,
    /// Draw the pointer REAPER draws for itself.
    ///
    /// The theme's `*_pan_knob_*` bitmaps have no pointer in them at all —
    /// they are a disc and a cap, and REAPER paints the indicator line
    /// over the top from the parameter's value. A panel that is not
    /// REAPER has to draw it, so it is a prop rather than part of the art:
    /// off, the exported PNGs stay exactly as they are audited; on, the
    /// knob reads as a knob.
    ///
    /// With the pointer on, the cap stays centred and the *pointer*
    /// carries the value — which is what REAPER does. The cap only slides
    /// when there is no pointer to do the work.
    #[props(default)]
    pub indicator: bool,
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
}

/// The pan knob — a dark disc with a lit cap sitting proud of it.
///
/// Every number here is measured off the source at sub-pixel precision, by
/// reading alpha coverage per row rather than thresholding to a silhouette:
///
/// ```text
/// small (24x25)   body  centre (12.0, 12.08) r 9.37
///                 cap   centre (12.0, 12.05) r 4.06
/// large (28x29)   body  centre (14.0, 14.05) r 11.22
///                 cap   centre (14.0, 14.05) r 4.80
/// ```
///
/// Note the body is *not* centred in the cell — it sits high, and the space
/// underneath is a drop shadow, which is why the earlier version's centred
/// disc read a pixel low.
///
/// The face is nearly flat: `#3c` at the top to `#31` at the bottom, over
/// 19 pixels. The previous version drew a strong radial gradient, which is
/// what made ours look like a blurred sphere against the source's moulded
/// disc — and it was light, because it derived from `hardware` unshaded.
#[component]
pub fn PanningKnob(props: PanProps) -> Element {
    let t = Theme::default();
    let (vw, vh) = if props.large {
        (28.0f32, 29.0f32)
    } else {
        (24.0f32, 25.0f32)
    };
    let (cx, cy, r) = if props.large {
        (14.0f32, 14.05f32, 11.22f32)
    } else {
        (12.0f32, 12.08f32, 9.37f32)
    };
    let (cap_cy, cap_r) = if props.large {
        (14.05f32, 4.80f32)
    } else {
        (12.05f32, 4.06f32)
    };

    // At rest the cap is dead centre — the knob shows pan by *sliding* it
    // across, not by pointing at a rim, so centre has to read as centre.
    let pos = props.position.clamp(-1.0, 1.0);
    let dx = if props.indicator {
        0.0
    } else {
        pos * (r - cap_r - 1.0)
    };
    // The pointer sweeps 135° each way from straight up, which is the
    // throw REAPER gives a pan control. Measured off a rendered knob: two
    // pixels wide on a 24-pixel body, running from just inside the rim
    // down to the cap, peaking at `#b3b3b3`.
    let sweep = pos * 135.0;
    let point_w = vw * 0.083;
    let point_top = cy - r + vh * 0.045;
    let point_bot = cy - cap_r - vh * 0.01;
    let point = t.chrome.hardware_mark.shade(0.11);

    // All neutral. The cap was `text_dim`, a light *blue*-grey — right for
    // a label on a panel, wrong for moulded plastic, where it reads as a
    // lit indicator rather than a part.
    let face = t.chrome.hardware;
    let rim = t.chrome.hardware_edge.shade(-0.45);
    // Neutral at *every* position, which is what the two lines above
    // decided and the next three used to contradict: off centre the cap
    // went accent, so any panned track showed a blue lamp where REAPER
    // shows the same moulded grey slid across. There is no accent variant
    // in the art to match — the source draws one cap and moves it.
    let cap = t.chrome.hardware_mark;

    rsx! {
        svg {
            width: "{props.width.unwrap_or(vw as u32)}",
            height: "{props.height.unwrap_or(vh as u32)}",
            view_box: "0 0 {vw} {vh}",
            xmlns: "http://www.w3.org/2000/svg",
            defs {
                linearGradient { id: "panface", x1: "0", y1: "0", x2: "0", y2: "1",
                    stop { offset: "0", stop_color: "{face.shade(-0.05).css()}" }
                    stop { offset: "1", stop_color: "{face.shade(-0.29).css()}" }
                }
                linearGradient { id: "pancap", x1: "0", y1: "0", x2: "0", y2: "1",
                    stop { offset: "0", stop_color: "{cap.shade(0.20).css()}" }
                    stop { offset: "1", stop_color: "{cap.shade(-0.08).css()}" }
                }
                // Both shadows are gradients rather than blur filters:
                // resvg renders these reliably at this size, and a filter
                // would cost a full offscreen pass per knob.
                radialGradient { id: "pandrop",
                    stop { offset: "0.90", stop_color: "#000000", stop_opacity: "0.15" }
                    stop { offset: "1", stop_color: "#000000", stop_opacity: "0" }
                }
                radialGradient { id: "pancapdrop",
                    stop { offset: "0.55", stop_color: "#000000", stop_opacity: "0.32" }
                    stop { offset: "1", stop_color: "#000000", stop_opacity: "0" }
                }
            }
            // Sits below the body, so only the part that clears it shows.
            // An ellipse, not a circle: the source has no shadow at all
            // beside the knob, and a circle wide enough to reach two rows
            // below it also reached two columns either side of it.
            ellipse {
                cx: "{cx}", cy: "{cy + 1.2}",
                // 2.15 down, not 1.9: shorter clips the source's own last
                // row of shadow, and longer adds one it does not have.
                // What is left is a single row at alpha 7 — the price of
                // a two-stop gradient standing in for a real blur.
                rx: "{r + 0.7}", ry: "{r + 2.15}",
                fill: "url(#pandrop)",
            }
            // Rim first, face inset — a stroke would centre itself on the
            // edge and put half the line outside the disc.
            circle { cx: "{cx}", cy: "{cy}", r: "{r}", fill: "{rim.css()}" }
            circle { cx: "{cx}", cy: "{cy}", r: "{r - 0.35}", fill: "url(#panface)" }
            circle {
                cx: "{cx + dx}", cy: "{cap_cy + 0.9}", r: "{cap_r + 1.1}",
                fill: "url(#pancapdrop)",
            }
            if props.indicator {
                rect {
                    x: "{cx - point_w * 0.5}", y: "{point_top}",
                    width: "{point_w}", height: "{point_bot - point_top}",
                    rx: "{point_w * 0.5}",
                    fill: "{point.css()}",
                    transform: "rotate({sweep} {cx} {cy})",
                }
            }
            circle {
                cx: "{cx + dx}", cy: "{cap_cy}", r: "{cap_r}",
                fill: "url(#pancap)",
            }
        }
    }
}

// ── fader cap: bevelled body, ribbed panel ───────────────────────────────

#[derive(Props, Clone, PartialEq)]
pub struct FaderCapProps {
    /// Track accent, which the cap picks up in REAPER's colour variants.
    #[props(default)]
    pub accent: Option<Color>,
    /// Which band of the source box to draw, when the caller is drawing
    /// the art as a stack of bands rather than whole.
    ///
    /// `mcp_volbg` is a nine-slice: its rail occupies rows 16..39 of 55
    /// because that is the stretchy middle, and REAPER lengthens *that*
    /// band to whatever the strip needs while the caps hold their size.
    /// Scaled naively to a taller box the whole drawing scales, rail and
    /// caps alike, which is the mistake the guides exist to prevent.
    ///
    /// `None` draws the whole source box — which is what the exporter
    /// wants, since it rasterises at the source size and the two are the
    /// same thing there. A panel drawing a 300px fader passes one pane per
    /// band; see [`crate::slice::NamedArt::stack`].
    #[props(default)]
    pub pane: Option<crate::slice::Pane>,
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
}

/// The fader cap — the ribbed plastic look, as geometry.
///
/// The rib count is fixed rather than derived from height: the original has
/// a set number of ridges, and deriving it would thin them out at one size
/// and crowd them at another, which is precisely what a vector version is
/// supposed to stop happening.
#[component]
pub fn VolumeFaderCap(props: FaderCapProps) -> Element {
    let t = Theme::default();
    let (vw, vh) = (27.0f32, 53.0f32);

    // Traced off `mcp_volthumb`, row by row down the centre column:
    //
    //     y5      #0e0e0e   top border
    //     y6-7    #696969   bevel, catching the light
    //     y8-12   #414141   body above the grip
    //     y13-39  ribs, alternating light and dark every row and
    //             brightening downward from #9d/#50 to #d9/#64
    //     y40-46  #2b2b2b   body below
    //     y47     #0b0b0b   bottom border
    //     y48-52  a soft drop shadow, fading to nothing
    //
    // The previous version drew two floating ribbed blocks with no body,
    // no border and no bevel — the cap read as a pair of grilles rather
    // than a moulded thumb, because those three are most of what makes it
    // look like an object at all.
    let grip = props.accent.unwrap_or(t.chrome.hardware_mark);
    let grip_id = vary("capgrip", &[&grip.css()]);
    let body = t.chrome.hardware;
    let edge = t.chrome.hardware_edge.shade(-0.35);

    // Fractions of the cell, all measured.
    // x2..x22 *inclusive* — the border pixel at x22 is part of the cap —
    // so the shape's right edge is at 23, not 21. Reading the last drawn
    // column as the edge left the body two pixels narrow.
    let (x0, x1) = (vw * 2.0 / 27.0, vw * 23.0 / 27.0);
    let (top, bot) = (vh * 5.0 / 53.0, vh * 48.0 / 53.0);
    let (gx0, gw) = (vw * 7.0 / 27.0, vw * 11.0 / 27.0);
    let (gy0, gy1) = (vh * 13.0 / 53.0, vh * 40.0 / 53.0);

    rsx! {
        svg {
            width: "{props.width.unwrap_or(27)}",
            height: "{props.height.unwrap_or(53)}",
            view_box: "0 0 {vw} {vh}",
            xmlns: "http://www.w3.org/2000/svg",
            defs {
                linearGradient { id: "capbody", x1: "0", y1: "0", x2: "0", y2: "1",
                    stop { offset: "0", stop_color: "{body.shade(0.06).css()}" }
                    stop { offset: "0.65", stop_color: "{body.shade(-0.02).css()}" }
                    stop { offset: "1", stop_color: "{body.shade(-0.32).css()}" }
                }
            }
            // The shadow it casts, below the body.
            rect {
                x: "{x0 + 1.0}", y: "{bot - 1.0}",
                width: "{x1 - x0 - 2.0}", height: "{vh - bot - 1.0}",
                rx: "{vw * 0.1}",
                fill: "#000000", fill_opacity: "0.20",
            }
            // Body, with a one-pixel border drawn as fill beneath it so
            // the face cannot bleed past the frame.
            rect {
                x: "{x0}", y: "{top}",
                width: "{x1 - x0}", height: "{bot - top}",
                rx: "{vw * 0.16}",
                fill: "{edge.css()}",
            }
            rect {
                x: "{x0 + 1.0}", y: "{top + 1.0}",
                width: "{x1 - x0 - 2.0}", height: "{bot - top - 2.0}",
                rx: "{vw * 0.13}",
                fill: "url(#capbody)",
            }
            // The lit bevel across the top — two rows, the brighter above.
            rect {
                x: "{x0 + 2.0}", y: "{top + 1.0}",
                width: "{x1 - x0 - 4.0}", height: "1.0",
                fill: "{body.shade(0.28).css()}",
            }
            rect {
                x: "{x0 + 2.0}", y: "{top + 2.0}",
                width: "{x1 - x0 - 4.0}", height: "1.0",
                fill: "{body.shade(0.16).css()}",
            }
            // The silver grip.
            //
            // Mapping it row by row shows it is not a comb of full-width
            // bands, which is what this drew:
            //
            //     y15  .....###########.....
            //     y16  .....###+++++###.....
            //     y26  .....................
            //
            // A light panel spanning x7..x17, and the grooves are *short
            // centre notches* — x10..x14 — so three columns of silver run
            // unbroken down each side. y26 is different again: a full
            // width dark row, the seam between the two halves. Drawing
            // every groove full width flattened the panel into a grille
            // and lost both the side rails and the seam.
            defs {
                // The grip takes the accent prop, so two caps with
                // different accents in one document are two gradients.
                linearGradient { id: "{grip_id}", x1: "0", y1: "0", x2: "0", y2: "1",
                    stop { offset: "0", stop_color: "{grip.shade(-0.03).css()}" }
                    stop { offset: "1", stop_color: "{grip.shade(0.34).css()}" }
                }
            }
            // The panel sits in a recess, so a ring of shadow runs round
            // it: the body reads ~63 elsewhere but 36 to 40 in the column
            // and row immediately outside the silver. Without it the grip
            // looks stuck on the front rather than set into the moulding.
            rect {
                x: "{gx0 - 1.0}", y: "{gy0 - 1.0}",
                width: "{gw + 2.0}", height: "{gy1 - gy0 + 2.0}",
                rx: "{vw * 0.09}",
                fill: "{body.shade(-0.43).css()}",
            }
            rect {
                x: "{gx0}", y: "{gy0}",
                width: "{gw}", height: "{gy1 - gy0}",
                rx: "{vw * 0.055}",
                fill: "url(#{grip_id})",
            }
            // Five notches, the seam, five more: eleven rows on alternate
            // lines from y16 to y36, with y26 the full-width split. This
            // ran to thirteen with the seam at the seventh, which put the
            // split a row late and carried two phantom notches past the
            // bottom of the panel.
            for i in 0..11i32 {
                {
                    let y = gy0 + vh * (3.0 + 2.0 * i as f32) / 53.0;
                    let seam = i == 5;
                    let down = i as f32 / 10.0;
                    // The seam is a good deal darker than the notches —
                    // 51 against their 85 and 96 — so it reads as the
                    // join between two halves rather than one more
                    // groove.
                    let ink_line = if seam {
                        grip.shade(-0.69)
                    } else {
                        grip.shade(-0.52 + 0.12 * down)
                    };
                    rsx! {
                        rect {
                            key: "{i}",
                            x: if seam { "{gx0}" } else { "{gx0 + gw * 0.27}" },
                            y: "{y}",
                            width: if seam { "{gw}" } else { "{gw * 0.46}" },
                            height: "{vh / 53.0}",
                            fill: "{ink_line.css()}",
                        }
                    }
                }
            }
        }
    }
}

/// The trough the cap runs in.
///
/// Traced off `mcp_volbg`: a **short** groove, x10..x12 and y14..y40 of a
/// 23x55 cell — three columns of black at alpha 60, 215, 60 — and
/// nothing else. Everything outside it is WALTER guide, columns 0 and 22
/// over rows 0..15 and 39..54, which is how REAPER stretches the trough
/// to whatever height a strip needs.
///
/// This drew a full-height line, which is the mistake the guides exist to
/// prevent: REAPER stretches the middle, so drawing the ends as art makes
/// them stretch too and the groove grows a solid bar at each end.
#[component]
pub fn VolumeFaderTrack(props: FaderCapProps) -> Element {
    let (vw, vh) = (23.0f32, 55.0f32);
    // Each band draws *its own slice* of the groove, in its own
    // coordinates. It used to draw the whole groove and let a viewBox
    // window it — which is the tidier idea and does not survive Blitz:
    // there a viewBox's min-y offset is ignored and nothing is clipped to
    // it, so all three bands painted the full 27-row groove and a tall
    // fader came out as three disconnected dashes. The intersection is
    // arithmetic every renderer agrees about.
    //
    // y14..y41, which is where the groove is *traced*, and deliberately not
    // the 16..39 the magenta guides mark. Those are two different facts: the
    // guides say which rows REAPER may stretch, the trace says where the
    // black actually is, and the source's groove runs a couple of rows into
    // the fixed caps at each end. Snapping the drawing to the guide changes
    // the exported PNG — it was the first thing this rewrite got wrong.
    let (rail_y, rail_h) = (vh * 14.0 / 55.0, vh * 27.0 / 55.0);
    let pane = props.pane.unwrap_or(crate::slice::Pane {
        view: (0.0, 0.0, vw, vh),
        grow: false,
    });
    let (px, py, pw, ph) = pane.view;
    // The width the caller asked for sets the scale, so a pane can size
    // itself without being told twice.
    let w = props.width.unwrap_or(vw as u32);
    let scale = w as f32 / vw;
    // Where the groove lands inside this band: the overlap of the traced
    // rows with the pane, expressed from the pane's own top.
    let (groove_y, groove_h) = {
        let (top, bottom) = (rail_y.max(py), (rail_y + rail_h).min(py + ph));
        (top - py, (bottom - top).max(0.0))
    };
    // A growing pane hands its height to the layout engine; a fixed one
    // states it. `preserveAspectRatio: none` because the rail is meant to
    // lengthen — letterboxing the stretch band is exactly the wrong answer.
    let sizing = if pane.grow {
        "flex:1; min-height:0; height:100%".to_string()
    } else {
        format!("flex:0 0 auto; height:{}px", ph * scale)
    };
    // An `<svg>` is a replaced element, so its `height` *attribute* beats
    // `flex:1` — a growing band that states the source band's height stays
    // that tall however long the strip is, which is what kept the rail
    // stuck at 27 rows beside a full-height meter. A caller that states a
    // height still gets it; the exporter always does.
    let height = match (pane.grow, props.height) {
        (_, Some(h)) => h.to_string(),
        (true, None) => "100%".to_string(),
        (false, None) => format!("{}", (ph * scale).round() as u32),
    };
    rsx! {
        svg {
            width: "{w}",
            height: "{height}",
            view_box: "0 0 {pw} {ph}",
            preserve_aspect_ratio: "none",
            style: "{sizing}; display:block",
            xmlns: "http://www.w3.org/2000/svg",
            // Slightly wider than a pixel and centred on x11.5, which is
            // what gives the source its soft shoulders at 60 either side
            // of a 215 core rather than one hard column.
            // The groove, clipped to this band and moved to its origin.
            if groove_h > 0.0 {
                rect {
                    x: "{vw * 10.8 / 23.0 - px}",
                    y: "{groove_y}",
                    width: "{vw * 1.4 / 23.0}",
                    height: "{groove_h}",
                    fill: "#000000", fill_opacity: "0.85",
                }
            }
        }
    }
}

// ── track panel: envelope mode ──────────────────────────────────────────

/// Which automation mode an envelope button is showing.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum EnvelopeMode {
    #[default]
    Off,
    Read,
    Write,
    Touch,
    Latch,
    Preview,
}

impl EnvelopeMode {
    /// The letter REAPER puts in the corner. `Off` has none.
    fn letter(self) -> Option<&'static str> {
        match self {
            Self::Off => None,
            Self::Read => Some("R"),
            Self::Write => Some("W"),
            Self::Touch => Some("T"),
            Self::Latch => Some("L"),
            Self::Preview => Some("P"),
        }
    }

    /// Measured off the source art.
    ///
    /// These are REAPER's automation-mode colours, not chrome: a user
    /// reads green as read and red as write the same way they read a
    /// traffic light, so they stay put when the rest of the theme is
    /// retinted. `Off` is the one that follows the palette.
    fn tint(self, t: &Theme) -> Color {
        match self {
            Self::Off => t.chrome.hardware_mark,
            Self::Read => Color::rgb(0x41, 0xce, 0x7c),
            Self::Write => Color::rgb(0xdb, 0x35, 0x50),
            Self::Touch => Color::rgb(0xff, 0xcb, 0x40),
            Self::Latch => Color::rgb(0xbd, 0x62, 0xdb),
            Self::Preview => Color::rgb(0x16, 0xa9, 0xfe),
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct EnvelopeProps {
    /// Draw the plate behind the mark. The track panel's controls sit on a
    /// scrim; the mixer's sit on the strip.
    #[props(default = true)]
    pub scrim: bool,
    #[props(default)]
    pub mode: EnvelopeMode,
    #[props(default = (20.0, 20.0))]
    pub cell: (f32, f32),
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    #[props(default)]
    pub at: Interaction,
}

/// The envelope button — a rising automation segment, and its mode letter.
///
/// The whole button is one colour at three opacities: a plate at 0.25 when
/// unlit and 0.35 when lit, a rim a shade under that, and the glyph at
/// 0.55 unlit and solid lit. Nothing here is a separate grey.
///
/// The glyph sits half a pixel lower with no letter than with one — the
/// unlit button centres its icon, and the lit ones shift up to make room —
/// so that offset is keyed off the letter rather than off the mode.
#[component]
pub fn EnvelopeButton(props: EnvelopeProps) -> Element {
    let t = Theme::default();
    let (vw, vh) = props.cell;
    let letter = props.mode.letter();
    let lit = letter.is_some();
    let tint = props.mode.tint(&t);
    let tint = match props.at {
        // Not `shade`: #41ce7c goes to #54e498 on hover, which moves red
        // by a tenth of its headroom and green by nearly half. That is a
        // scale, not a mix toward white.
        Interaction::Hover => lift(tint, 0.15),
        _ => tint,
    };

    // The plate is the track panel's usual scrim — flat black at a low
    // opacity — not a wash of the mode colour. Tinting it made every lit
    // mode glow, and turned the alpha-weighted mean from #08 into #3e.
    //
    // The mixer does not want it at all. A scrim reads as a plate over a
    // track panel's colour; over the mixer strip's own #262626 it is just
    // a darker box sitting on the grey, which is what made the button look
    // like a hole in the column. Same split the FX pill makes between the
    // two families, for the same reason.
    let plate = if props.scrim {
        if lit { 0.35 } else { 0.25 }
    } else {
        0.0
    };
    let glyph = if lit { 1.0 } else { 0.73 };
    // Down half a pixel when there is no letter to make room for.
    let dy = if lit { 0.0 } else { 0.6 };

    rsx! {
        svg {
            width: "{props.width.unwrap_or(vw as u32)}",
            height: "{props.height.unwrap_or(vh as u32)}",
            view_box: "0 0 {vw} {vh}",
            xmlns: "http://www.w3.org/2000/svg",
            rect {
                x: "0", y: "0", width: "{vw}", height: "{vh}", rx: "2.4",
                fill: "#000000", fill_opacity: "{plate * 0.7}",
            }
            rect {
                x: "1", y: "1", width: "{vw - 2.0}", height: "{vh - 2.0}",
                rx: "1.6",
                fill: "#000000", fill_opacity: "{plate}",
            }
            g { opacity: "{glyph}", fill: "{tint.css()}", stroke: "{tint.css()}",
                // The run: in flat from the left, up to the right, out
                // flat again. Thinner than the handles it joins.
                path {
                    d: "M 2 {12.5 + dy} H 7 L 13 {5.5 + dy} H 18",
                    fill: "none",
                    stroke_width: "1.1",
                }
                rect { x: "5.3", y: "{10.3 + dy}", width: "3.4", height: "3.4", rx: "0.3" }
                rect { x: "11.3", y: "{4.3 + dy}", width: "3.4", height: "3.4", rx: "0.3" }
            }
            if let Some(letter) = letter {
                text {
                    x: "14.4", y: "13.4",
                    text_anchor: "middle", dominant_baseline: "central",
                    font_family: "Fira Sans, DejaVu Sans, sans-serif",
                    font_weight: "700", font_size: "7.8",
                    fill: "{tint.css()}",
                    "{letter}"
                }
            }
        }
    }
}

// ── track panel: phase invert ───────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
pub struct PhaseProps {
    /// Inverted — the lit blue state.
    #[props(default)]
    pub inverted: bool,
    /// The REAPER image this draws: the box its art is drawn at, and which
    /// band of that box takes up slack when it is drawn bigger — see
    /// [`NamedArt`]. Deliberately without a default, because one component
    /// serves several images at several sizes.
    pub art: NamedArt,
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    #[props(default)]
    pub at: Interaction,
}

/// The phase button — a slashed circle on a rounded plate.
///
/// The plate is nearly round: 16 wide by 15.6 tall with a corner radius of
/// 6, which at this size reads as a squircle rather than either a circle
/// or a button. Its rim is the one thing the two states do differently —
/// unlit it is the plate a shade down, lit it is flat #171717, because a
/// blue plate shaded down is still blue and the source's is black.
#[component]
pub fn PhaseButton(props: PhaseProps) -> Element {
    let t = Theme::default();
    let (vw, vh) = props.art.source;

    let plate = if props.inverted {
        Color::rgb(0x16, 0xa9, 0xfe)
    } else {
        t.chrome.hardware.shade(-0.05)
    };
    let plate = match props.at {
        Interaction::Hover => plate.shade(if props.inverted { 0.30 } else { 0.13 }),
        _ => plate,
    };
    let rim = if props.inverted {
        t.chrome.hardware_edge
    } else {
        plate.shade(-0.24)
    };
    // Bright on the dark plate, dark on the blue one: the glyph is
    // whichever of the two reads against what is behind it.
    let ink = if props.inverted {
        t.chrome.hardware_edge
    } else {
        t.chrome.hardware_mark.shade(0.36)
    };

    // The plate is 15.6 tall and sits 2 up from the bottom of whatever
    // cell it is in — 2.4 down in the track panel's 20 rows, 0.4 in the
    // mixer's 18. One number, both families.
    let top = vh - 17.6;
    let (cx, cy) = (vw * 0.5, top + 7.4);
    rsx! {
        svg {
            width: "{props.width.unwrap_or(vw as u32)}",
            height: "{props.height.unwrap_or(vh as u32)}",
            view_box: "0 0 {vw} {vh}",
            xmlns: "http://www.w3.org/2000/svg",
            rect {
                x: "0", y: "{top}", width: "{vw}", height: "15.6", rx: "7",
                fill: "{rim.css()}",
            }
            rect {
                x: "0.9", y: "{top + 0.9}", width: "{vw - 1.8}", height: "13.8",
                rx: "6.1",
                fill: "{plate.css()}",
            }
            circle {
                cx: "{cx}", cy: "{cy}", r: "3.6",
                fill: "none", stroke: "{ink.css()}", stroke_width: "1.0",
            }
            line {
                x1: "{cx - 3.3}", y1: "{cy + 3.1}",
                x2: "{cx + 3.3}", y2: "{cy - 3.1}",
                stroke: "{ink.css()}", stroke_width: "1.0",
            }
        }
    }
}

// ── track panel: record mode ────────────────────────────────────────────

/// What a track's record-mode button is showing.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum RecordMode {
    /// Disabled — a plain X.
    #[default]
    Off,
    /// Record input: an arrow running into a bracket.
    Input,
    /// Record output: a bracket with an arrow running out of it.
    Output,
}

#[derive(Props, Clone, PartialEq)]
pub struct RecordModeProps {
    #[props(default)]
    pub mode: RecordMode,
    #[props(default = (20.0, 20.0))]
    pub cell: (f32, f32),
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    #[props(default)]
    pub at: Interaction,
}

/// The record-mode button.
///
/// Input and output are the same drawing mirrored about the centre — an
/// arrow and a bracket, with the arrow pointing into the bracket for input
/// and out of it for output. Writing it once and flipping `dir` keeps the
/// two from drifting apart.
///
/// The three states do not share an opacity: the source's plate is 0.25
/// behind the X and the input arrow but 0.35 behind the output one, and
/// the glyphs run 0.85, 0.55 and 0.80. Those are measured, not a ramp.
#[component]
pub fn RecordModeButton(props: RecordModeProps) -> Element {
    let (vw, vh) = props.cell;
    let (cx, cy) = (vw * 0.5, 9.5f32);

    let (plate, glyph) = match props.mode {
        RecordMode::Off => (0.25f32, 0.85f32),
        RecordMode::Input => (0.25, 0.55),
        RecordMode::Output => (0.35, 0.80),
    };
    let boost = match props.at {
        Interaction::Hover => 1.35,
        _ => 1.0,
    };
    // +1 draws the bracket on the right and the arrow flying into it;
    // -1 mirrors both.
    let dir = if matches!(props.mode, RecordMode::Input) {
        1.0f32
    } else {
        -1.0
    };
    let spine = cx + dir * 5.5;
    let arm = cx + dir * 1.0;
    let tip = cx + dir * 1.5;
    let tail = cx - dir * 6.0;

    rsx! {
        svg {
            width: "{props.width.unwrap_or(vw as u32)}",
            height: "{props.height.unwrap_or(vh as u32)}",
            view_box: "0 0 {vw} {vh}",
            xmlns: "http://www.w3.org/2000/svg",
            rect {
                x: "0", y: "0", width: "{vw}", height: "{vh}", rx: "2.4",
                fill: "#000000", fill_opacity: "{plate}",
            }
            g {
                opacity: "{(glyph * boost).min(1.0)}",
                stroke: "#ffffff",
                fill: "none",
                if matches!(props.mode, RecordMode::Off) {
                    // Two bars crossing, not four arms from the centre:
                    // the source's strokes run corner to corner unbroken.
                    g { stroke_width: "2.0",
                        line {
                            x1: "{cx - 4.0}", y1: "{cy - 4.0}",
                            x2: "{cx + 4.0}", y2: "{cy + 4.0}",
                        }
                        line {
                            x1: "{cx + 4.0}", y1: "{cy - 4.0}",
                            x2: "{cx - 4.0}", y2: "{cy + 4.0}",
                        }
                    }
                } else {
                    g { stroke_width: "1.0",
                        path {
                            d: "M {arm} {cy - 5.0} H {spine} V {cy + 5.0} H {arm}",
                        }
                        line { x1: "{tail}", y1: "{cy}", x2: "{tip}", y2: "{cy}" }
                        path {
                            d: "M {tip - dir * 2.4} {cy - 2.4} L {tip} {cy}
                                L {tip - dir * 2.4} {cy + 2.4}",
                        }
                    }
                }
            }
        }
    }
}

// ── track panel: folder compact indicator ───────────────────────────────

/// How compact a folder's child tracks are drawn.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum FolderCompact {
    /// Full size — a triangle pointing down.
    #[default]
    Off,
    /// Small — a ramp.
    Small,
    /// Tiny — a triangle pointing right.
    Tiny,
}

#[derive(Props, Clone, PartialEq)]
pub struct FolderCompactProps {
    #[props(default)]
    pub state: FolderCompact,
    #[props(default = (17.0, 13.0))]
    pub cell: (f32, f32),
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    #[props(default)]
    pub at: Interaction,
}

/// The folder-compact indicator — a small mark over a fading wash.
///
/// The wash is the interesting part: it is not a plate but a gradient,
/// white at 0.15 along the top edge and gone by two thirds down, with a
/// hard rule near the bottom. Drawing it as a flat plate made the strip
/// look banded, because the rows either side of it are this same wash at
/// different strengths.
#[component]
pub fn FolderCompactButton(props: FolderCompactProps) -> Element {
    let (vw, vh) = props.cell;
    // Hover lifts the wash as well as the mark — the source's top row
    // goes 38 to 70 — so a version that only brightened the glyph left
    // the whole strip looking a shade flat next to it.
    let ink = match props.at {
        Interaction::Hover => 0.69f32,
        _ => 0.44,
    };
    // The hover wash is not the resting one turned up. Resting fades to
    // nothing — `#e9e9e9` at 0.149 down to `#131313` at 0.008, which
    // composites 35 to 0 — while hover stays bright the whole way and
    // turns back up at the foot: 67, down to 43, back to 38. Scaled from
    // one set of stops the hover cell came out a flat 37 levels dark.
    let hovered = props.at == Interaction::Hover;
    let wash: [(&str, &str, f32); 3] = if hovered {
        [
            ("0", "#f4f4f4", 0.275),
            ("0.40", "#d6d6d6", 0.200),
            ("1", "#ffffff", 0.149),
        ]
    } else {
        [
            ("0", "#e9e9e9", 0.149),
            ("0.55", "#4a4a4a", 0.043),
            ("1", "#0a0a0a", 0.008),
        ]
    };
    // Two washes, so a hovered button beside a resting one is two ids —
    // keyed on every stop, not just the first, per vary()'s contract.
    let wash_key: String = wash
        .iter()
        .map(|(at, hex, a)| format!("{at}{hex}{a}"))
        .collect();
    let wash_id = vary("fcompwash", &[&wash_key]);

    // Traced: down-triangle, ramp, right-triangle, all in the same box.
    let glyph = match props.state {
        FolderCompact::Off => format!(
            // Top edge at 3.1, not the 2.8 the coverage suggests: at 2.8
            // the triangle catches a row the source leaves clear, which
            // costs more in the bounding box than the tenth of a level it
            // gains in the average.
            "M {} 3.1 H {} L {} 8.0 Z",
            vw * 0.288,
            vw * 0.671,
            vw * 0.5
        ),
        FolderCompact::Small => format!("M {} 8.0 H {} V 2.8 Z", vw * 0.26, vw * 0.71),
        FolderCompact::Tiny => format!("M {} 1.7 V 8.7 L {} 5.2 Z", vw * 0.353, vw * 0.618),
    };

    rsx! {
        svg {
            width: "{props.width.unwrap_or(vw as u32)}",
            height: "{props.height.unwrap_or(vh as u32)}",
            view_box: "0 0 {vw} {vh}",
            xmlns: "http://www.w3.org/2000/svg",
            defs {
                linearGradient { id: "{wash_id}", x1: "0", y1: "0", x2: "0", y2: "1",
                    for (i, (at, hex, a)) in wash.iter().enumerate() {
                        stop {
                            key: "w{i}", offset: "{at}",
                            stop_color: "{hex}", stop_opacity: "{a}",
                        }
                    }
                }
            }
            rect { x: "0", y: "0", width: "{vw}", height: "{vh - 3.0}", fill: "url(#{wash_id})" }
            // Pure black over pure white, both at low alpha — not the
            // mid-greys these were. `#9a9a9a` at 0.345 composites 53
            // levels light against the source's `#000000` at the same
            // alpha, which is the single largest error in the family.
            rect { x: "0", y: "{vh - 2.0}", width: "{vw}", height: "1", fill: "#000000", fill_opacity: "0.345" }
            rect { x: "0", y: "{vh - 1.0}", width: "{vw}", height: "1", fill: "#ffffff", fill_opacity: "0.094" }
            path { d: "{glyph}", fill: "#ffffff", fill_opacity: "{ink}" }
        }
    }
}

// ── track panel: input FX ───────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
pub struct FxInProps {
    /// Something is in the input chain — the lit blue state.
    #[props(default)]
    pub loaded: bool,
    #[props(default = (29.0, 20.0))]
    pub cell: (f32, f32),
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    #[props(default)]
    pub at: Interaction,
}

/// The input-FX button — "FX" on the track panel's usual scrim.
#[component]
pub fn FxInButton(props: FxInProps) -> Element {
    let t = Theme::default();
    let (vw, vh) = props.cell;
    // #46b9fe is `accent` exactly. Empty is the same wash the rest of the
    // panel's disabled marks use.
    let ink = if props.loaded {
        t.chrome.accent
    } else {
        t.chrome.hardware_mark.shade(-0.30)
    };
    let ink = match props.at {
        Interaction::Hover => lift(ink, 0.22),
        _ => ink,
    };
    let solid = if props.loaded { 1.0f32 } else { 0.55 };

    rsx! {
        svg {
            width: "{props.width.unwrap_or(vw as u32)}",
            height: "{props.height.unwrap_or(vh as u32)}",
            view_box: "0 0 {vw} {vh}",
            xmlns: "http://www.w3.org/2000/svg",
            rect {
                x: "0", y: "0", width: "{vw}", height: "{vh}", rx: "2.2",
                fill: "#000000", fill_opacity: "0.35",
            }
            text {
                x: "{vw * 0.5}", y: "{vh * 0.47}",
                text_anchor: "middle", dominant_baseline: "central",
                font_family: "Fira Sans, DejaVu Sans, sans-serif",
                font_weight: "700", font_size: "8.6",
                fill: "{ink.css()}",
                fill_opacity: "{solid}",
                "FX"
            }
        }
    }
}

// ── track panel: folder state ───────────────────────────────────────────

/// Where a track sits in a folder.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum FolderState {
    /// Not a folder — a folder icon and two plus signs.
    #[default]
    Off,
    /// A folder — an opaque block from x18 with two double-bar marks.
    On,
    /// Last in the folder — a corner wedge and two down arrows.
    Last,
}

#[derive(Props, Clone, PartialEq)]
pub struct FolderProps {
    #[props(default)]
    pub state: FolderState,
    #[props(default = (54.0, 14.0))]
    pub cell: (f32, f32),
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    #[props(default)]
    pub at: Interaction,
}

/// The folder-state strip.
///
/// Not a sprite of three pointer states — three marks side by side in one
/// image, which is why its measured cell is the whole 54 rather than a
/// third of it.
///
/// `On` is the odd one: an opaque #333333 block from x18 to the right
/// edge, carrying two pairs of lighter bars. It read as blank at first
/// because its PNG is greyscale-plus-alpha rather than RGBA, and a reader
/// expecting four channels per pixel silently matched none of it. Worth
/// remembering — "the source is empty" is a conclusion that deserves a
/// second look.
#[component]
pub fn FolderButton(props: FolderProps) -> Element {
    let t = Theme::default();
    let (vw, vh) = props.cell;
    let ink = match props.at {
        Interaction::Hover => 0.46,
        _ => 0.33,
    };
    let last = props.state == FolderState::Last;

    rsx! {
        svg {
            width: "{props.width.unwrap_or(vw as u32)}",
            height: "{props.height.unwrap_or(vh as u32)}",
            view_box: "0 0 {vw} {vh}",
            xmlns: "http://www.w3.org/2000/svg",
            if props.state == FolderState::On {
                rect {
                    x: "18", y: "0", width: "{vw - 18.0}", height: "{vh}",
                    fill: "{t.chrome.surface.shade(-0.18).css()}",
                }
                for (i, at) in [27.9f32, 45.9].iter().enumerate() {
                    g { key: "on{i}", fill: "{t.chrome.hardware.shade(0.20).css()}",
                        rect { x: "{at - 2.6}", y: "1.3", width: "5.3", height: "4.2" }
                        rect { x: "{at - 2.6}", y: "7.3", width: "5.3", height: "4.2" }
                    }
                }
            } else {
                // The wedge only appears on the last child — it is the
                // corner that closes the folder's bracket.
                if last {
                    path {
                        d: "M 0 3.6 L 18 13.6 H 0 Z",
                        fill: "{t.chrome.hardware.shade(-0.22).css()}",
                    }
                }
                g { fill: "#1d1d1d", fill_opacity: "{ink}",
                    // A folder: a tab, then the body under it.
                    path { d: "M 5 2 H 9 V 4 H 5 Z" }
                    rect { x: "5", y: "4", width: "9", height: "5" }
                    for (i, at) in [27.5f32, 45.5].iter().enumerate() {
                        g { key: "{i}",
                            if last {
                                // A down arrow: a stem over a head.
                                path {
                                    d: "M {at - 2.5} 0 H {at + 2.5} V 3 H {at + 5.5}
                                        L {at} 9.5 L {at - 5.5} 3 H {at - 2.5} Z",
                                }
                            } else {
                                // A plus.
                                rect { x: "{at - 1.5}", y: "0", width: "3", height: "11" }
                                rect { x: "{at - 5.5}", y: "4", width: "11", height: "3" }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── transport bar ───────────────────────────────────────────────────────

/// One of the transport bar's buttons.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TransportGlyph {
    /// Jump to the start — a bar with a triangle pointing into it.
    Home,
    /// Previous marker — `Home` under a flag.
    Previous,
    Stop,
    #[default]
    Play,
    Pause,
    /// Next marker — `End` under a flag.
    Next,
    /// Jump to the end.
    End,
    /// Arm — a ring.
    Record,
    /// Arm for the selected item — a ring struck through.
    RecordItem,
    /// Arm for the loop — a ring in brackets.
    RecordLoop,
    /// Repeat — two arrows chasing each other.
    Repeat,
    /// Locked play — a padlock beside the triangle.
    PlaySync,
}

impl TransportGlyph {
    /// The colour the plate takes when the button is on.
    ///
    /// Measured off the `_on` variants. `Repeat` is the odd one: its plate
    /// never changes and only the arrows light, which is why this is per
    /// glyph rather than a single "engaged" colour.
    fn lit(self, _t: &Theme) -> Option<Bevel> {
        match self {
            Self::Play | Self::PlaySync => Some(Bevel {
                rim: Color::rgb(0x7f, 0xd6, 0xfb),
                rim_bot: Color::rgb(0x47, 0xb8, 0xfb),
                edge: Color::rgb(0x36, 0x9d, 0xe1),
                centre: Color::rgb(0x4d, 0xbd, 0xfb),
            }),
            Self::Pause => Some(Bevel {
                rim: Color::rgb(0xf8, 0xb3, 0x12),
                // Amber alone runs *lighter* along the bottom rim than the
                // top, where every other plate here runs darker.
                rim_bot: Color::rgb(0xf9, 0xd7, 0x2e),
                edge: Color::rgb(0xbd, 0x8e, 0x27),
                centre: Color::rgb(0xd1, 0xa8, 0x3b),
            }),
            Self::Record | Self::RecordItem | Self::RecordLoop => Some(Bevel {
                rim: Color::rgb(0xf7, 0x4c, 0x5c),
                rim_bot: Color::rgb(0xe1, 0x3a, 0x51),
                edge: Color::rgb(0xde, 0x39, 0x39),
                centre: Color::rgb(0xfc, 0x57, 0x57),
            }),
            _ => None,
        }
    }
}

/// The four colours a lit transport plate is built from.
///
/// All measured, none derived. The relationship between a rim and the
/// face inside it is not a constant: record's interior is *brighter* than
/// its rim in the red channel and pause's is thirty levels darker in all
/// three, so a single offset cannot serve both.
struct Bevel {
    rim: Color,
    rim_bot: Color,
    edge: Color,
    centre: Color,
}

#[derive(Props, Clone, PartialEq)]
pub struct TransportProps {
    #[props(default)]
    pub glyph: TransportGlyph,
    /// Engaged — plays, records, repeats.
    #[props(default)]
    pub on: bool,
    /// `transport_*` is 36x26; repeat's pair are 32x24.
    #[props(default = (36.0, 26.0))]
    pub cell: (f32, f32),
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    #[props(default)]
    pub at: Interaction,
}

/// A transport button — one plate, twelve glyphs.
///
/// The plate is the whole cell rather than an inset shape: a 1px border,
/// a face falling #3b3b3b to #2e2e2e over the button's height, and a
/// separator row along the bottom that belongs to the bar rather than to
/// the button. Corners are square — they read as rounded at 350% because
/// a one-pixel border does, not because they are.
///
/// Repeat sits on a darker plate than the rest (#262626 to #1d1d1d) and
/// never lights it; only its arrows change colour. Everything else lights
/// the plate and darkens the glyph, except record, which lights the plate
/// *and* turns its ring white.
#[component]
pub fn TransportButton(props: TransportProps) -> Element {
    let t = Theme::default();
    let (vw, vh) = props.cell;
    let repeat = props.glyph == TransportGlyph::Repeat;
    let h = t.chrome.hardware;
    let lit = props.on.then(|| props.glyph.lit(&t)).flatten();

    // The unlit face is nearly flat — 50 down to 47 over twenty rows —
    // under a single lighter bevel row. The gradient this had was reading
    // that bevel as the top of the face and running #3b3b3b to #2e2e2e,
    // four times the actual fall.
    let (face_top, face_bot) = match () {
        _ if repeat => (
            t.chrome.hardware.shade(-0.40),
            t.chrome.hardware.shade(-0.54),
        ),
        _ => (
            t.chrome.hardware.shade(-0.21),
            t.chrome.hardware.shade(-0.26),
        ),
    };
    // Hover *adds*, it does not scale: 50/47 goes to 70/67, twenty levels
    // on both ends, where a proportional lift moves the top three times
    // as far as the bottom. Pressed takes five back off.
    let shift = match props.at {
        Interaction::Hover => 20.0,
        Interaction::Pressed => -5.0,
        Interaction::Normal => 0.0,
    };
    let (face_top, face_bot) = (offset(face_top, shift), offset(face_bot, shift));
    // A transport bar is a row of these in different states and colours,
    // so every def whose content depends on the button is derived.
    let face_id = vary("trface", &[&face_top.css(), &face_bot.css()]);
    let lit_id = lit
        .as_ref()
        .map(|b| vary("trlit", &[&b.edge.css(), &b.centre.css()]))
        .unwrap_or_default();
    let litside_id = vary("trlitside", &[&format!("{vw}")]);
    let border = t.chrome.hardware_edge.shade(0.05);
    // Record is the last button of the transport cluster, and its lit art
    // caps the group: square on the left, rounded on the right, with the
    // corners beyond it transparent. Play, pause and locked play do not
    // do this — they are in the middle of the row — so it is per glyph
    // rather than a property of being lit.
    let end_cap = lit.is_some()
        && matches!(
            props.glyph,
            TransportGlyph::Record | TransportGlyph::RecordItem | TransportGlyph::RecordLoop
        );

    // One plate layer: square on the left, rounded on the right by `cap`
    // where this button ends the cluster. A clip-path round the whole
    // group was the obvious way to do it and resvg dropped the group,
    // plate and all; building the radius into each shape renders.
    let cap = if end_cap { 3.5f32 } else { 0.0 };
    let open_left = lit.is_some()
        && matches!(
            props.glyph,
            TransportGlyph::RecordItem | TransportGlyph::RecordLoop
        );
    let plate = |x: f32, y: f32, h: f32| {
        // Inset on the right by as much as on the left, which the first
        // version forgot: every layer ran to the full width and each one
        // overhung the border by a column.
        let right = vw - x;
        // Except on the left of the two that carry no border there: the
        // lit `record_item` and `record_loop` run their face out to
        // column 0, because the button to their left continues into
        // them. Drawing the border there put a dark rule down the side —
        // a hundred levels, every row, about a fifth of the image's error.
        //
        // Plain lit `record` is *not* one of them, despite capping the
        // group on the right exactly as they do. Grouping it with them
        // by end-cap alone made it worse by two.
        let x = if open_left { (x - 1.0).max(0.0) } else { x };
        if cap <= 0.0 {
            return format!("M {x} {y} H {right} V {} H {x} Z", y + h);
        }
        let r = (cap - x).max(0.5);
        format!(
            "M {x} {y} H {} A {r} {r} 0 0 1 {right} {} V {} A {r} {r} 0 0 1 {} {} H {x} Z",
            right - r,
            y + r,
            y + h - r,
            right - r,
            y + h,
        )
    };

    // The glyph reads against whatever is behind it.
    let ink = match (props.glyph, props.on) {
        (
            TransportGlyph::Record | TransportGlyph::RecordItem | TransportGlyph::RecordLoop,
            false,
        ) => t.signal.rec.shade(-0.10),
        (
            TransportGlyph::Record | TransportGlyph::RecordItem | TransportGlyph::RecordLoop,
            true,
        ) => t.chrome.hardware_mark.shade(0.94),
        (TransportGlyph::Repeat, true) => Color::rgb(0xf8, 0xcf, 0x5e),
        (_, true) => t.chrome.hardware_edge,
        (_, false) => t.chrome.hardware_mark.shade(0.32),
    };

    // A lit ring throws a soft light inside itself: the plate reads
    // #ff5f5f outside a lit record button and #ff7272 within its ring,
    // which is white at an eighth over the red. The unlit variants differ
    // by one level, so it is the lamp and not the moulding.
    let halo = match (props.glyph, props.on) {
        (TransportGlyph::Record | TransportGlyph::RecordLoop, true) => Some(3.0f32),
        (TransportGlyph::RecordItem, true) => Some(6.0),
        _ => None,
    };

    // Traced boxes, all centred on (18, 13) of a 36x26 cell bar repeat,
    // which is centred on (16, 12) of its 32x24 one.
    // Repeat's glyph sits half a pixel above the centre of its cell —
    // rows 6..17 of 24 — where every other glyph is centred.
    // Repeat's glyph is the one that is not centred in its cell: rows
    // 6..17 of 24 and columns 11..22 of 32, so half a pixel up and half
    // a pixel right of where the rest sit.
    // Repeat's glyph is centred half a pixel right of its cell's middle
    // and level with it: rows 6..17 of 24 and columns 11..22 of 32.
    // Precomputed, because an rsx attribute takes an identifier or a
    // simple expression — a call with arithmetic in its arguments parses
    // but does not come out the other side.
    let plate_border = plate(0.0, 1.0, vh - 2.0);
    let plate_rim = plate(1.0, 2.0, vh - 4.0);
    let plate_rim_bot = plate(1.0, vh - 3.0, 1.0);
    // Inset from the rim on *both* axes. It had been inset only
    // vertically, so the rim's bright left and right columns were painted
    // straight over by the face — the source keeps a full-height column
    // of `#7ed5fa` down each side of a lit button and this covered them.
    let plate_face = plate(2.0, 3.0, vh - 6.0);

    let (cx, cy) = if repeat {
        (vw * 0.5 + 0.5, vh * 0.5)
    } else {
        (vw * 0.5, vh * 0.5)
    };
    let d = match props.glyph {
        // 8x8, dead centre.
        TransportGlyph::Stop => format!("M {} {} h 8 v 8 h -8 z", cx - 4.0, cy - 4.0),
        // Two 3px bars with a 2px gap.
        TransportGlyph::Pause => format!(
            "M {} {} h 3 v 8 h -3 z M {} {} h 3 v 8 h -3 z",
            cx - 4.0,
            cy - 4.0,
            cx + 1.0,
            cy - 4.0
        ),
        TransportGlyph::Play => format!(
            "M {} {} V {} L {} {} Z",
            cx - 4.0,
            cy - 5.0,
            cy + 5.0,
            cx + 4.5,
            cy
        ),
        // A triangle running into a bar, and its mirror.
        TransportGlyph::End | TransportGlyph::Next => format!(
            "M {} {} V {} L {} {} Z M {} {} h 2.5 v 10 h -2.5 z",
            cx - 5.0,
            cy - 5.0,
            cy + 5.0,
            cx + 2.0,
            cy,
            cx + 2.0,
            cy - 5.0
        ),
        TransportGlyph::Home | TransportGlyph::Previous => format!(
            "M {} {} V {} L {} {} Z M {} {} h 2.5 v 10 h -2.5 z",
            cx + 5.0,
            cy - 5.0,
            cy + 5.0,
            cx - 2.0,
            cy,
            cx - 4.5,
            cy - 5.0
        ),
        // A ring, and the same ring struck through or bracketed.
        TransportGlyph::Record => ring(cx, cy, 6.0, 3.0),
        // A two-pixel band, not a four: outer 8 and inner 6, with an
        // 8 by 4 bar through the middle. Read off the lit variant, where
        // the white separates from the red cleanly — the band came out
        // twice its width and the bar half its height.
        TransportGlyph::RecordItem => format!(
            "{} M {} {} h 8 v 4 h -8 z",
            ring(cx, cy, 8.0, 6.0),
            cx - 4.0,
            cy - 2.0
        ),
        // The brackets have a top arm only, and it is chamfered rather
        // than square: a one-pixel stem twelve rows tall with a triangle
        // at its head running down and inward. Drawn with arms top *and*
        // bottom — which is what `[o]` looks like, and what the icon reads
        // as at a glance — it carried a whole flange the source does not.
        TransportGlyph::RecordLoop => format!(
            "{} M {} {} h 1.2 v 12 h -1.2 z M {} {} h 3.6 l -3.6 3.6 z \
             M {} {} h 1.2 v 12 h -1.2 z M {} {} h -3.6 l 3.6 3.6 z",
            ring(cx, cy, 6.0, 3.0),
            cx - 10.0,
            cy - 6.0,
            cx - 10.0,
            cy - 6.0,
            cx + 8.8,
            cy - 6.0,
            cx + 10.0,
            cy - 6.0
        ),
        // Two arrowheads. The bands they cap are stroked separately.
        //
        // Traced off the lit variant, where the amber separates cleanly
        // from the plate in the green channel. Each half is a band of
        // about 135 degrees — not the half turn a first reading suggests
        // — and the head sits at one end only: the upper band's is on the
        // right pointing down, the lower band's on the left pointing up.
        // Drawn as two symmetric arcs with heads laid over both ends it
        // closed into a plain ring.
        //
        // The band also widens where it meets its head, which a stroke of
        // constant width cannot do — so the flare has to come from how far
        // the head overlaps the band's end. Ten combinations of base width
        // and reach were measured; this one wins by half a level over its
        // nearest rival and by a full level over a head large enough to
        // look right on its own.
        TransportGlyph::Repeat => format!(
            "M {} {} L {} {} L {} {} Z M {} {} L {} {} L {} {} Z",
            cx + 1.80,
            cy - 0.90,
            cx + 6.40,
            cy - 0.90,
            cx + 4.10,
            cy - 4.60,
            cx - 1.80,
            cy + 0.90,
            cx - 6.40,
            cy + 0.90,
            cx - 4.10,
            cy + 4.60
        ),
        // A padlock beside the triangle.
        //
        // The shackle is a proper arch standing clear above the body, not
        // a bump on top of it: five columns wide with a one-pixel wall,
        // rising four rows from the body's shoulder. Drawn as a small arc
        // tucked into the body it sat three rows too low and read as a
        // smudge — the one part of this glyph that says "locked".
        //
        // Body x8..x14 and y12..y16 of a 36x26 cell; shackle x9..x13 from
        // y8; triangle x19..x27 over y8..y17.
        TransportGlyph::PlaySync => format!(
            "M {} {} h 7 v 5 h -7 z \
             M {} {} V {} A 2.5 2.5 0 0 1 {} {} V {} H {} V {} \
             A 1.5 1.5 0 0 0 {} {} V {} Z \
             M {} {} V {} L {} {} Z",
            cx - 10.0,
            cy - 1.0,
            cx - 9.0,
            cy - 1.0,
            cy - 2.5,
            cx - 4.0,
            cy - 2.5,
            cy - 1.0,
            cx - 5.0,
            cy - 2.5,
            cx - 8.0,
            cy - 2.5,
            cy - 1.0,
            cx + 1.0,
            cy - 5.0,
            cy + 5.0,
            cx + 9.5,
            cy
        ),
    };

    rsx! {
        svg {
            width: "{props.width.unwrap_or(vw as u32)}",
            height: "{props.height.unwrap_or(vh as u32)}",
            view_box: "0 0 {vw} {vh}",
            xmlns: "http://www.w3.org/2000/svg",
            defs {
                // No `trcap` clip here any more: a clip-path round the lit
                // group was abandoned (resvg dropped the group) in favour
                // of building the radius into each shape, and its def
                // outlived every reference — found dead by the id audit.
                linearGradient { id: "trcycleedge", x1: "0", y1: "0", x2: "1", y2: "0",
                    stop { offset: "0", stop_color: "#000000", stop_opacity: "0.15" }
                    stop { offset: "0.62", stop_color: "#000000", stop_opacity: "0" }
                }
                linearGradient { id: "trcycle", x1: "0", y1: "0", x2: "0", y2: "1",
                    // 34 at the top, up to 40 by a fifth of the way down,
                    // back to 34 by four fifths and 29 at the foot.
                    stop { offset: "0", stop_color: "{h.shade(-0.46).css()}" }
                    stop { offset: "0.22", stop_color: "{h.shade(-0.37).css()}" }
                    stop { offset: "0.83", stop_color: "{h.shade(-0.46).css()}" }
                    stop { offset: "1", stop_color: "{h.shade(-0.54).css()}" }
                }
                linearGradient { id: "{face_id}", x1: "0", y1: "0", x2: "0", y2: "1",
                    stop { offset: "0", stop_color: "{face_top.css()}" }
                    stop { offset: "1", stop_color: "{face_bot.css()}" }
                }
                if let Some(b) = &lit {
                    linearGradient { id: "{lit_id}", x1: "0", y1: "0", x2: "0", y2: "1",
                        // Four stops, because the source holds its centre
                        // rather than passing through it: the plateau runs
                        // from y7 to y18 of twenty rows, and a gradient
                        // that only touches it at the midpoint averages
                        // eleven levels dark over the whole button.
                        stop { offset: "0", stop_color: "{b.edge.css()}" }
                        stop { offset: "0.22", stop_color: "{b.centre.css()}" }
                        stop { offset: "0.78", stop_color: "{b.centre.css()}" }
                        stop { offset: "1", stop_color: "{b.edge.css()}" }
                    }
                    // The lit face is bevelled *across* as well as down —
                    // 226 at each side column against 255 three columns
                    // in, the same fall the vertical stops carry. Drawn
                    // with the vertical ramp alone the button read as a
                    // lit strip rather than a lit key.
                    linearGradient { id: "{litside_id}", x1: "0", y1: "0", x2: "1", y2: "0",
                        stop { offset: "0", stop_color: "#000000", stop_opacity: "0.114" }
                        stop { offset: "{3.0 / vw}", stop_color: "#000000", stop_opacity: "0" }
                        stop { offset: "{1.0 - 3.0 / vw}", stop_color: "#000000", stop_opacity: "0" }
                        stop { offset: "1", stop_color: "#000000", stop_opacity: "0.114" }
                    }
                }
            }
            if repeat {
                // The cycle button is the left end of a pill, not a
                // button in its own right: it fills its cell edge to edge
                // with no border, no bevel row and no separator, because
                // the readout well butts straight up against it and the
                // two read as one shape. Given the standard transport
                // plate it came out two rows short at the top and one at
                // the bottom, sitting in the bar like a tile instead of
                // running into its neighbour.
                //
                // Its three cells are also identical — the plate never
                // lifts and the glyph never changes — so `at` does not
                // reach it.
                // Rounded on the left, square on the right: this is the
                // *cap* of the pill, and the readout well continues it.
                // Drawn as a plain rect it had no left edge at all — the
                // one thing that tells you where the pill begins.
                path {
                    d: "M 4.2 0 H {vw} V {vh} H 4.2
                        A 4.2 4.2 0 0 1 0 {vh - 4.2} V 4.2
                        A 4.2 4.2 0 0 1 4.2 0 Z",
                    fill: "url(#trcycle)",
                }
                // And an inner shadow down that edge: 34 at x0 against 40
                // four columns in, which is a black wash at 0.15 fading
                // out by x3.
                path {
                    d: "M 4.2 0 H 6 V {vh} H 4.2
                        A 4.2 4.2 0 0 1 0 {vh - 4.2} V 4.2
                        A 4.2 4.2 0 0 1 4.2 0 Z",
                    fill: "url(#trcycleedge)",
                }
            } else {
                path { d: "{plate_border}", fill: "{border.css()}" }
            }
            if repeat {
            } else if let Some(b) = &lit {
                // A lit plate is a bevel, not a wash. Down its centre the
                // colour runs #7fd6fb, #369de1, up to #4dbdfb through the
                // middle, back down to #369de1 and out on #47b8fb: a
                // bright rim top and bottom with the face inset dark
                // against it. Painted as one flat field it read as a
                // sticker rather than a lit button.
                path { d: "{plate_rim}", fill: "{b.rim.css()}" }
                path { d: "{plate_rim_bot}", fill: "{b.rim_bot.css()}" }
                path { d: "{plate_face}", fill: "url(#{lit_id})" }
                path { d: "{plate_face}", fill: "url(#{litside_id})" }
            } else {
                // One lighter row under the top border, which every
                // ReaperTips control has.
                rect {
                    x: "1", y: "2", width: "{vw - 2.0}", height: "1",
                    fill: "{offset(face_top, 9.0).css()}",
                }
                rect {
                    x: "1", y: "3", width: "{vw - 2.0}", height: "{vh - 5.0}",
                    fill: "url(#{face_id})",
                }
            }
            // The bar's own separator, not the button's edge — it runs the
            // full width where the border is inset by a pixel.
            if !repeat {
                rect {
                    x: "0", y: "{vh - 1.0}", width: "{vw}", height: "1",
                    fill: "{t.chrome.hardware.shade(-0.11).css()}",
                }
            }
            if repeat {
                path {
                    // 195 to 330 degrees over the top, and the same
                    // turned through half a circle underneath.
                    d: "M {cx - 4.51} {cy - 1.64} A 4.8 4.8 0 0 1 {cx + 3.93} {cy - 2.75}
                        M {cx + 4.51} {cy + 1.64} A 4.8 4.8 0 0 1 {cx - 3.93} {cy + 2.75}",
                    fill: "none",
                    stroke: "{ink.css()}",
                    // 2.2 measured against 2.5, 2.8 and 3.1:
                    // the source's band looks thicker than it is
                    // because its ends are squared off, not because
                    // the stroke is wide.
                    stroke_width: "2.2",
                }
            }
            if let Some(r) = halo {
                circle {
                    cx: "{cx}", cy: "{cy}", r: "{r}",
                    fill: "#ffffff", fill_opacity: "0.12",
                }
            }
            path { d: "{d}", fill: "{ink.css()}", fill_rule: "evenodd" }
        }
    }
}

/// Shift every channel by a constant number of levels.
///
/// Distinct from [`lift`], which scales. Some of this theme's states move
/// by a fixed amount — the transport's hover adds twenty levels to a face
/// whose ends are only three apart — and a scale cannot express that.
fn offset(c: Color, by: f32) -> Color {
    let one = |v: u8| (v as f32 + by).clamp(0.0, 255.0) as u8;
    Color::rgb(one(c.r), one(c.g), one(c.b))
}

/// An annulus as one even-odd path — the shape every record button uses.
fn ring(cx: f32, cy: f32, outer: f32, inner: f32) -> String {
    format!(
        "M {} {cy} A {outer} {outer} 0 1 0 {} {cy} A {outer} {outer} 0 1 0 {} {cy} Z \
         M {} {cy} A {inner} {inner} 0 1 1 {} {cy} A {inner} {inner} 0 1 1 {} {cy} Z",
        cx - outer,
        cx + outer,
        cx - outer,
        cx - inner,
        cx + inner,
        cx - inner,
    )
}

// ── transport bar: the panels behind the buttons ────────────────────────

/// Which piece of transport furniture to draw.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TransportPart {
    /// The bar itself — a rounded panel the buttons sit on.
    #[default]
    Panel,
    /// The readout's recessed well.
    Status,
    /// The same well when the engine is in trouble: a flat red.
    StatusError,
    /// The tempo readout's pair of wells.
    Bpm,
    /// The play-rate slider's groove.
    SpeedTrack,
    /// The play-rate slider's thumb.
    SpeedThumb,
    /// The ring around the play-rate knob.
    KnobRing,
    /// The timebase toggle showing beats — a barrel.
    TimebaseBeat,
    /// The timebase toggle showing time — a clock.
    TimebaseTime,
    /// Three of REAPER's images are wholly transparent. Drawing nothing
    /// is the faithful answer, and saying so here is better than leaving
    /// them traced and wondering later why they trace to nothing.
    Empty,
}

#[derive(Props, Clone, PartialEq)]
pub struct TransportPartProps {
    #[props(default)]
    pub part: TransportPart,
    #[props(default = (200.0, 67.0))]
    pub cell: (f32, f32),
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    #[props(default)]
    pub at: Interaction,
}

/// The transport bar's panels, wells and slider parts.
///
/// These are nine-slices: REAPER stretches them, so what matters is the
/// edges and the first row or two inside them, not the middle. The panel
/// is the clearest case — a two-row dark band, a two-row bevel, then a
/// face that falls three levels over sixty rows and would be flat if the
/// bevel had not been mistaken for its top.
#[component]
pub fn TransportPanel(props: TransportPartProps) -> Element {
    let t = Theme::default();
    let (vw, vh) = props.cell;
    let h = t.chrome.hardware;
    let tb_plate = if props.at == Interaction::Hover {
        h.shade(0.03)
    } else {
        h.shade(-0.19)
    }
    .css();

    rsx! {
        svg {
            width: "{props.width.unwrap_or(vw as u32)}",
            height: "{props.height.unwrap_or(vh as u32)}",
            view_box: "0 0 {vw} {vh}",
            xmlns: "http://www.w3.org/2000/svg",
            defs {
                linearGradient { id: "trpanel", x1: "0", y1: "0", x2: "0", y2: "1",
                    // The bevel is the first two rows, not the top of the
                    // face: #4b4b4b for two rows, then #414141 settling to
                    // #3e3e3e over the remaining sixty.
                    stop { offset: "0", stop_color: "{h.shade(0.19).css()}" }
                    stop { offset: "0.045", stop_color: "{h.shade(0.03).css()}" }
                    stop { offset: "1", stop_color: "{h.shade(-0.02).css()}" }
                }
                linearGradient { id: "trwell", x1: "0", y1: "0", x2: "0", y2: "1",
                    stop { offset: "0", stop_color: "{h.shade(-0.44).css()}" }
                    stop { offset: "0.12", stop_color: "{h.shade(-0.37).css()}" }
                    stop { offset: "1", stop_color: "{h.shade(-0.40).css()}" }
                }
                linearGradient { id: "trknobband", x1: "0", y1: "0", x2: "0", y2: "1",
                    stop { offset: "0", stop_color: "{h.shade(0.58).css()}" }
                    stop { offset: "1", stop_color: "{h.shade(0.26).css()}" }
                }
                linearGradient { id: "trknobwell", x1: "0", y1: "0", x2: "0", y2: "1",
                    stop { offset: "0", stop_color: "{h.shade(-0.13).css()}" }
                    stop { offset: "1", stop_color: "{h.shade(-0.25).css()}" }
                }
                linearGradient { id: "trthumb", x1: "0", y1: "0", x2: "0", y2: "1",
                    // 167 at the top, dipping to 143 a quarter of the way
                    // down, then back to 175 — a shallow trough, not the
                    // bright-dark-bright of a moulded cap. Read as the
                    // latter it averaged twenty levels light.
                    // A bevel row at 167, then 136 climbing steadily to
                    // 175 at the foot. Read as a trough — bright, dark,
                    // bright — it came out twenty levels light and lit
                    // from the wrong end.
                    stop { offset: "0", stop_color: "{h.shade(0.54).css()}" }
                    stop { offset: "0.09", stop_color: "{h.shade(0.38).css()}" }
                    stop { offset: "1", stop_color: "{h.shade(0.58).css()}" }
                }
            }
            match props.part {
                TransportPart::Empty => rsx! {},
                TransportPart::Panel => rsx! {
                    // Guides live at x0 and y0, so the panel starts at 1.
                    rect {
                        x: "1", y: "1", width: "{vw - 2.0}", height: "{vh - 2.0}",
                        rx: "5",
                        fill: "{h.shade(-0.27).css()}",
                    }
                    rect {
                        x: "1", y: "3", width: "{vw - 2.0}", height: "{vh - 4.0}",
                        rx: "4",
                        fill: "url(#trpanel)",
                    }
                },
                TransportPart::Status => rsx! {
                    // A well, not a panel: lit down its left and top-inner
                    // and darker along its right and bottom, which is what
                    // makes it read as cut into the bar.
                    rect {
                        x: "1", y: "1", width: "{vw - 2.0}", height: "{vh - 2.0}",
                        rx: "1.5",
                        fill: "{h.shade(-0.48).css()}",
                    }
                    rect {
                        x: "1", y: "1", width: "{vw - 3.0}", height: "{vh - 3.0}",
                        rx: "1.5",
                        fill: "url(#trwell)",
                    }
                    // The lit column runs rows 5 to 22 and stops: the
                    // last three rows are the well's dark floor, and
                    // taking the highlight through them put a bright line
                    // down the whole seam where the source ends it.
                    rect {
                        x: "1", y: "5", width: "1", height: "18",
                        fill: "{h.shade(0.00).css()}",
                    }
                },
                TransportPart::StatusError => rsx! {
                    rect {
                        x: "0", y: "0", width: "{vw}", height: "{vh}",
                        fill: "{t.signal.mute.shade(-0.36).css()}",
                    }
                },
                TransportPart::Bpm => rsx! {
                    // Two wells side by side in the right two thirds of
                    // the cell — x32..x61 and x62..x89 of 92, rows 3..22 —
                    // with the left one lighter. The left third is empty;
                    // spreading them across the whole cell put twenty
                    // columns of well where the source has nothing.
                    rect {
                        x: "32", y: "3", width: "30", height: "20",
                        fill: "{h.shade(0.00).css()}",
                    }
                    rect {
                        x: "33", y: "4", width: "28", height: "18",
                        fill: "{h.shade(0.03).css()}",
                    }
                    rect {
                        x: "62", y: "3", width: "28", height: "20",
                        fill: "{h.shade(-0.19).css()}",
                    }
                },
                TransportPart::SpeedTrack => rsx! {
                    // Two rows of groove in a 21-row cell, the rest guide.
                    // The same shape as the mixer's fader trough, and the
                    // same reason: REAPER stretches the middle.
                    rect {
                        x: "2", y: "11", width: "1", height: "2",
                        fill: "{h.shade(-0.32).css()}",
                    }
                },
                TransportPart::SpeedThumb => rsx! {
                    // x5..x16, y5..y21 of a 22x28 cell — narrower and
                    // taller than a proportional guess makes it.
                    // A soft black surround, not a border: the frame
                    // columns read alpha 51 with 16 outside them, so it
                    // is a shadow at a fifth strength over two pixels.
                    // Drawn solid it was the heaviest thing in the cell.
                    rect {
                        x: "4", y: "4", width: "14", height: "18", rx: "2",
                        fill: "#000000", fill_opacity: "0.07",
                    }
                    rect {
                        x: "5", y: "5", width: "12", height: "16", rx: "1.5",
                        fill: "#000000", fill_opacity: "0.20",
                    }
                    rect {
                        x: "6", y: "6", width: "10", height: "14", rx: "1",
                        fill: "url(#trthumb)",
                    }
                },
                TransportPart::TimebaseBeat | TransportPart::TimebaseTime => rsx! {
                    // No plate at rest; hover and pressed each get one,
                    // and they are the only thing the three cells differ
                    // by. Both glyphs are 11 by 11 at #adadad, centred on
                    // (16, 10) of a 33-wide cell.
                    if props.at != Interaction::Normal {
                        rect {
                            x: "0", y: "1", width: "{vw}", height: "{vh - 2.0}",
                            rx: "3",
                            fill: "{tb_plate}",
                        }
                    }
                    if props.part == TransportPart::TimebaseTime {
                        circle {
                            cx: "16.5", cy: "10.5", r: "5",
                            fill: "none",
                            stroke: "{h.shade(0.58).css()}",
                            stroke_width: "1.2",
                        }
                        path {
                            d: "M 16.5 7.3 V 10.9 H 19.2",
                            fill: "none",
                            stroke: "{h.shade(0.58).css()}",
                            stroke_width: "1.2",
                        }
                    } else {
                        // A barrel: a filled cap top and bottom with four
                        // staves between them, which is why rows 10 to 12
                        // show only four columns of ink.
                        g { fill: "{h.shade(0.58).css()}",
                            ellipse { cx: "16.5", cy: "7.6", rx: "5.5", ry: "2.5" }
                            ellipse { cx: "16.5", cy: "14.3", rx: "5.5", ry: "2.0" }
                            for (i, x) in [11.5f32, 14.5, 18.5, 21.5].iter().enumerate() {
                                rect {
                                    key: "{i}",
                                    x: "{x - 0.55}", y: "7",
                                    width: "1.1", height: "7",
                                }
                            }
                        }
                    }
                },
                TransportPart::KnobRing => rsx! {
                    // Centred half a pixel below the middle of its cell,
                    // which is what a 34-row cell holding a 27-row ring
                    // works out to.
                    // A wide bright band round a dark well, both lit from
                    // above: the band runs 174 down to 113 over the ring's
                    // height and the interior 55 down to 47. Drawn as a
                    // thin flat stroke it was a third of the width and a
                    // single value.
                    //
                    // The band is a filled annulus rather than a stroke
                    // because resvg flattens a gradient on a stroke to its
                    // average — the same reason the record ring is one.
                    circle {
                        cx: "15.9", cy: "15.4", r: "11.0",
                        fill: "url(#trknobwell)",
                    }
                    path {
                        d: "{ring(15.9, 15.4, 12.2, 9.9)}",
                        fill: "url(#trknobband)",
                        fill_rule: "evenodd",
                    }
                },
            }
        }
    }
}

// ── panel sliders: the horizontal ones ──────────────────────────────────

/// Which part of a track-panel slider to draw.
///
/// The mixer's volume fader runs vertically and has its own components.
/// Everything here runs across: the track panel's volume, and both panels'
/// pan and width.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SliderPart {
    /// The track panel's volume groove — a flat plate with a centre tick.
    #[default]
    VolumeTrough,
    /// Its cap, which is the mixer's fader cap turned on its side.
    VolumeThumb,
    /// The pan and width groove — a black slot with rounded ends.
    PanTrough,
    /// Their cap: small, bright, with a dark line down the middle.
    PanThumb,
    /// The mixer's folder mark — a folder on its own, no plus or arrow
    /// beside it, at half black over the strip.
    MixerFolder,
    /// The mixer's "last in folder" mark: a wedge in the bottom-right
    /// corner, where the track panel puts its on the left.
    MixerFolderLast,
}

#[derive(Props, Clone, PartialEq)]
pub struct SliderProps {
    #[props(default)]
    pub part: SliderPart,
    /// The REAPER image this draws: the box its art is drawn at, and which
    /// band of that box takes up slack when it is drawn bigger — see
    /// [`NamedArt`]. Deliberately without a default, because one component
    /// serves several images at several sizes.
    pub art: NamedArt,
    /// Rows to shift the pan cap down. `tcp_widththumb` is the same
    /// drawing as the other three thumbs one row lower — 164 pixels
    /// different and every one of them that offset.
    #[props(default = 0.0)]
    pub drop: f32,
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    #[props(default)]
    pub at: Interaction,
}

/// A track-panel slider part.
///
/// The troughs are nine-slices REAPER stretches along their length, so what
/// is drawn here is the ends and one row of the middle — the same rule the
/// mixer's fader trough follows, and for the same reason: art in the part
/// that stretches gets stretched.
#[component]
pub fn PanelSlider(props: SliderProps) -> Element {
    let t = Theme::default();
    let (vw, vh) = props.art.source;
    let h = t.chrome.hardware;

    rsx! {
        svg {
            width: "{props.width.unwrap_or(vw as u32)}",
            height: "{props.height.unwrap_or(vh as u32)}",
            view_box: "0 0 {vw} {vh}",
            xmlns: "http://www.w3.org/2000/svg",
            defs {
                // Across, not down: this cap lies on its side, so its
                // moulding runs left to right with a seam at the middle.
                linearGradient { id: "slthumb", x1: "0", y1: "0", x2: "1", y2: "0",
                    stop { offset: "0", stop_color: "{h.shade(0.40).css()}" }
                    stop { offset: "0.22", stop_color: "{h.shade(0.60).css()}" }
                    stop { offset: "1", stop_color: "{h.shade(0.41).css()}" }
                }
                // The pan cap's face and its two edge columns. Both carry
                // their shoulder rows in the same gradient, as a pair of
                // stops a fiftieth apart — the top and bottom rows of the
                // cap are sixty levels down from the face, not a ramp
                // into it.
                linearGradient { id: "slpan", x1: "0", y1: "0", x2: "0", y2: "1",
                    stop { offset: "0", stop_color: "{h.shade(-0.095).css()}" }
                    stop { offset: "0.124", stop_color: "{h.shade(-0.095).css()}" }
                    stop { offset: "0.126", stop_color: "{h.shade(0.641).css()}" }
                    stop { offset: "0.874", stop_color: "{h.shade(0.682).css()}" }
                    stop { offset: "0.876", stop_color: "{h.shade(-0.079).css()}" }
                    stop { offset: "1", stop_color: "{h.shade(-0.079).css()}" }
                }
                linearGradient { id: "slpanedge", x1: "0", y1: "0", x2: "0", y2: "1",
                    stop { offset: "0", stop_color: "{h.shade(-0.365).css()}" }
                    stop { offset: "0.124", stop_color: "{h.shade(-0.365).css()}" }
                    stop { offset: "0.126", stop_color: "{h.shade(0.406).css()}" }
                    stop { offset: "0.874", stop_color: "{h.shade(0.427).css()}" }
                    stop { offset: "0.876", stop_color: "{h.shade(-0.365).css()}" }
                    stop { offset: "1", stop_color: "{h.shade(-0.365).css()}" }
                }
                radialGradient { id: "slpanhalo",
                    cx: "6.5", cy: "12.5", r: "8.4",
                    gradient_units: "userSpaceOnUse",
                    stop { offset: "0.62", stop_color: "#000000", stop_opacity: "0.30" }
                    stop { offset: "1", stop_color: "#000000", stop_opacity: "0" }
                }
            }
            match props.part {
                SliderPart::VolumeTrough => rsx! {
                    rect {
                        x: "1", y: "1", width: "{vw - 2.0}", height: "{vh - 2.0}",
                        fill: "{h.shade(-0.40).css()}",
                    }
                    // The centre tick, two pixels of it, in the one row
                    // REAPER does not stretch.
                    rect {
                        x: "6", y: "11", width: "2", height: "1",
                        fill: "#000000",
                    }
                },
                SliderPart::VolumeThumb => rsx! {
                    rect {
                        x: "5", y: "6", width: "17", height: "16", rx: "1",
                        fill: "{t.chrome.hardware_edge.shade(-0.35).css()}",
                    }
                    rect {
                        x: "6", y: "7", width: "15", height: "14",
                        fill: "url(#slthumb)",
                    }
                    // The seam, one column, a little left of centre.
                    rect {
                        x: "13", y: "7", width: "1", height: "14",
                        fill: "{t.chrome.hardware_edge.shade(-0.35).css()}",
                    }
                },
                SliderPart::PanTrough => rsx! {
                    // A black slot with rounded ends, inset a pixel all
                    // round: rows 2..8 of eleven, whatever the length.
                    rect {
                        x: "1", y: "2", width: "{vw - 2.0}", height: "7",
                        rx: "3.5",
                        fill: "#000000",
                    }
                },
                SliderPart::MixerFolder => rsx! {
                    // A tab and a body, x5..x13 and y6..y12, at half
                    // black. The track panel draws the same folder beside
                    // two other marks; the mixer draws it alone.
                    g { fill: "#000000", fill_opacity: "0.50",
                        path { d: "M 5 6 H 9 V 8 H 5 Z" }
                        rect { x: "5", y: "8", width: "9", height: "5" }
                    }
                },
                SliderPart::MixerFolderLast => rsx! {
                    // A right-angled wedge filling the bottom-right
                    // corner, hypotenuse running from (11, 21) up to
                    // (21, 11).
                    path {
                        d: "M 21 10.5 V 21 H 10.5 Z",
                        fill: "#000000",
                    }
                },
                SliderPart::PanThumb => rsx! {
                    // A bright cap with a soft shadow round it — not the
                    // marker-with-a-tapering-tail it had been drawn as.
                    // The tail was rows 15 to 20 read as opaque geometry;
                    // they are black at alpha 0.42 falling to 0.05, which
                    // is a shadow, and rows 5 and 6 are the same thing
                    // above.
                    //
                    // Left as a tail *and* squeezed into a third of its
                    // width by `states`, this was the worst image in the
                    // set at 28 mean levels.
                    g { transform: "translate(0 {props.drop})",
                        ellipse { cx: "6.5", cy: "12.5", rx: "6.4", ry: "8.4",
                            fill: "url(#slpanhalo)" }
                        rect { x: "0", y: "7", width: "13", height: "8",
                            fill: "#000000", fill_opacity: "0.07" }
                        rect { x: "1", y: "7", width: "11", height: "8",
                            fill: "{h.shade(-0.90).css()}", fill_opacity: "0.81" }
                        rect { x: "2", y: "7", width: "9", height: "8",
                            fill: "url(#slpanedge)" }
                        rect { x: "3", y: "7", width: "7", height: "8",
                            fill: "url(#slpan)" }
                        // The seam, dead centre, running the cap's full
                        // height including both shoulder rows.
                        rect { x: "6", y: "7", width: "1", height: "8",
                            fill: "{h.shade(-0.683).css()}" }
                    }
                },
            }
        }
    }
}

// ── panel plates: the nine-slices behind everything ─────────────────────

/// One horizontal band of a plate: `(y, height, fill, alpha)`.
///
/// Every background in the mixer, track panel and envelope panel is a
/// stack of these — a few rows of one colour over a few rows of another,
/// which REAPER stretches vertically and horizontally to fill whatever it
/// is behind. None of them is a gradient; they are all flat bands, and the
/// ones that look like gradients are two bands a few levels apart.
pub type Band = (f32, f32, Color, f32);

/// A vertical mark laid over the bands: `(x, width, y, height, fill)`.
///
/// Bands alone cannot say "and there is an accent bar down the right-hand
/// edge", which is exactly what marks the selected envelope panel — two
/// columns of `#46b9fe` between two of `#242424`, running the body's
/// height and stopping short of the separator row. Read as a band-only
/// plate that stripe simply vanished, and with it the only thing on the
/// image that says the panel is selected at all.
pub type Stripe = (f32, f32, f32, f32, Color);

#[derive(Props, Clone, PartialEq)]
pub struct PlateProps {
    /// Bands, top to bottom, in cell rows.
    pub bands: Vec<Band>,
    /// Vertical marks drawn over the bands. Empty for all but one plate.
    #[props(default)]
    pub stripes: Vec<Stripe>,
    /// The REAPER image this draws: the box its art is drawn at, and which
    /// band of that box takes up slack when it is drawn bigger — see
    /// [`NamedArt`]. Deliberately without a default, because one component
    /// serves several images at several sizes.
    pub art: NamedArt,
    /// Columns of margin, left and right. Asymmetric because some of
    /// these plates are: the mixer's selected icon background leaves one
    /// column for a rule on the left and one bare column on the right.
    #[props(default = (0.0, 0.0))]
    pub inset: (f32, f32),
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    #[props(default)]
    pub at: Interaction,
}

/// A panel background.
///
/// Drawing these as components rather than shipping the bitmaps is what
/// lets the panel follow the palette: every colour below is a shade of the
/// theme's `hardware` grey, and the measured value it came from is written
/// beside it in the table that feeds this.
#[component]
pub fn PanelPlate(props: PlateProps) -> Element {
    let (vw, vh) = props.art.source;
    rsx! {
        svg {
            width: "{props.width.unwrap_or(vw as u32)}",
            height: "{props.height.unwrap_or(vh as u32)}",
            view_box: "0 0 {vw} {vh}",
            xmlns: "http://www.w3.org/2000/svg",
            for (i, band) in props.bands.iter().enumerate() {
                rect {
                    key: "{i}",
                    x: "{props.inset.0}", y: "{band.0}",
                    width: "{vw - props.inset.0 - props.inset.1}", height: "{band.1}",
                    fill: "{band.2.css()}",
                    fill_opacity: "{band.3}",
                }
            }
            for (i, s) in props.stripes.iter().enumerate() {
                rect {
                    key: "s{i}",
                    x: "{s.0}", y: "{s.2}", width: "{s.1}", height: "{s.3}",
                    fill: "{s.4.css()}",
                }
            }
        }
    }
}

// ── list rows: the FX and send lists ────────────────────────────────────

/// One pill of a list strip: `(top, bottom, alpha)`.
///
/// Flat where top and bottom are equal, which most of them are — only the
/// FX list's normal state carries a gradient.
pub type ListPill = (Color, Color, f32);

#[derive(Props, Clone, PartialEq)]
pub struct ListStripProps {
    /// The three pills, top to bottom: normal, hover, pressed.
    pub pills: Vec<ListPill>,
    /// The REAPER image this draws: the box its art is drawn at, and which
    /// band of that box takes up slack when it is drawn bigger — see
    /// [`NamedArt`]. Deliberately without a default, because one component
    /// serves several images at several sizes.
    pub art: NamedArt,
    /// First pill's top row, and the pitch between them.
    #[props(default = (2.0, 17.0))]
    pub rows: (f32, f32),
    /// Pill height.
    #[props(default = 15.0)]
    pub pill: f32,
    /// Columns of margin each side. The FX list insets by one and the
    /// send list does not, which is the sort of thing that only shows up
    /// as a whole family scoring 0.95 when it had been scoring 1.0.
    #[props(default = 1.0)]
    pub inset: f32,
    /// A line along each pill's foot — the MIDI hardware send's blue.
    #[props(default)]
    pub edge: Option<Color>,
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    #[props(default)]
    pub at: Interaction,
}

/// A list strip — three states stacked *vertically*.
///
/// Which is the thing to know about these: every other sprite in this
/// theme lays its states out side by side, and the cell detector only
/// looks for horizontal periods, so it reports one cell of the full width
/// and is right to. The three pills are one drawing.
#[component]
pub fn ListStrip(props: ListStripProps) -> Element {
    let (vw, vh) = props.art.source;
    let (top, pitch) = props.rows;
    // The pill colours come off the props, so two strips' gradients are
    // two sets of ids — the index alone repeats on every strip.
    let pill_ids: Vec<String> = props
        .pills
        .iter()
        .enumerate()
        .map(|(i, pill)| vary(&format!("pill{i}"), &[&pill.0.css(), &pill.1.css()]))
        .collect();
    rsx! {
        svg {
            width: "{props.width.unwrap_or(vw as u32)}",
            height: "{props.height.unwrap_or(vh as u32)}",
            view_box: "0 0 {vw} {vh}",
            xmlns: "http://www.w3.org/2000/svg",
            defs {
                for (i, pill) in props.pills.iter().enumerate() {
                    linearGradient { key: "g{i}", id: "{pill_ids[i]}",
                        x1: "0", y1: "0", x2: "0", y2: "1",
                        stop { offset: "0", stop_color: "{pill.0.css()}" }
                        stop { offset: "1", stop_color: "{pill.1.css()}" }
                    }
                }
            }
            for (i, pill) in props.pills.iter().enumerate() {
                g { key: "{i}",
                    rect {
                        x: "{props.inset}", y: "{top + pitch * i as f32}",
                        width: "{vw - props.inset * 2.0}", height: "{props.pill}",
                        rx: "5",
                        fill: "url(#{pill_ids[i]})",
                        fill_opacity: "{pill.2}",
                    }
                    if let Some(edge) = props.edge {
                        rect {
                            x: "{props.inset}",
                            y: "{top + pitch * i as f32 + props.pill - 1.0}",
                            width: "{vw - props.inset * 2.0}", height: "1",
                            rx: "0.5",
                            fill: "{edge.css()}",
                        }
                    }
                }
            }
        }
    }
}

// ── envelope panel ──────────────────────────────────────────────────────
//
// The envelope panel's buttons share a habit worth stating once: **their
// pressed cell is their normal cell.** Every one of `arm`, `bypass`,
// `hide` and the two lit plates is byte-identical in cells 0 and 2, so
// only hover moves. The unlit `learn` and `parammod` are the exception —
// pressed there gains a fill their normal state does not have — which is
// why `at` is read per control rather than through the shared `ink`.

/// The mark on an envelope-panel plate button.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum EnvcpGlyph {
    /// Parameter learn — a thick ring cut open across the lower left.
    #[default]
    Learn,
    /// Parameter modulation — one period of a wave.
    ParamMod,
}

#[derive(Props, Clone, PartialEq)]
pub struct EnvcpPlateProps {
    #[props(default)]
    pub glyph: EnvcpGlyph,
    /// Lit: the mark and the tab go accent, and the plate gains a fill.
    #[props(default)]
    pub lit: bool,
    #[props(default = (30.0, 20.0))]
    pub cell: (f32, f32),
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    #[props(default)]
    pub at: Interaction,
}

/// A plate button on the envelope panel — learn and parameter-modulation.
///
/// One drawing with two marks. The plate is a 1px rounded outline with a
/// tab hanging off the top edge, and the *unlit normal* state is the only
/// one with no fill at all: it reads as an outline over whatever the
/// panel behind it is, which is why cell 0 of `envcp_learn` is 362 pixels
/// of nothing and cell 2 of the same image is 350 pixels of `#2f2f2f`.
#[component]
pub fn EnvcpPlate(props: EnvcpPlateProps) -> Element {
    let t = Theme::default();
    let (vw, vh) = props.cell;
    let mark = t.chrome.hardware_mark;

    // Measured per cell rather than derived: the two toggle states darken
    // on press in opposite directions. Unlit goes transparent → #3a3a3a →
    // #2f2f2f; lit goes #2d2d2d → #373737 → #2d2d2d.
    let fill = match (props.lit, props.at) {
        (false, Interaction::Normal) => None,
        (false, Interaction::Hover) => Some(t.chrome.hardware.shade(-0.086)),
        (false, Interaction::Pressed) => Some(t.chrome.hardware.shade(-0.254)),
        (true, Interaction::Hover) => Some(t.chrome.hardware.shade(-0.127)),
        (true, _) => Some(t.chrome.hardware.shade(-0.286)),
    };
    let hovered = props.at == Interaction::Hover;
    // The outline lifts a few levels under the pointer and nowhere else.
    let edge = if hovered {
        t.chrome.hardware_edge.shade(0.032)
    } else {
        t.chrome.hardware_edge.shade(0.021)
    };
    let (tab, ink) = match (props.lit, hovered) {
        (true, false) => (t.chrome.accent, t.chrome.accent),
        (true, true) => (offset(t.chrome.accent, 15.0), offset(t.chrome.accent, 15.0)),
        (false, false) => (mark.shade(-0.049), mark.shade(-0.068)),
        (false, true) => (mark.shade(0.161), mark.shade(0.129)),
    };
    // Learn is a filled shape, ParamMod a stroked one — flat in both
    // cases, which is the only reason the stroke is safe here: resvg
    // averages a gradient on one away.
    let learn = props.glyph == EnvcpGlyph::Learn;
    let glyph_fill = if learn { ink.css() } else { "none".to_string() };
    let glyph_stroke = if learn { 0.0f32 } else { 1.75 };
    let plate_fill = fill.map(|c| c.css()).unwrap_or_else(|| "none".to_string());
    let path = match props.glyph {
        // Not a ring: a solid disc of r 4.90 about (15, 11) with a
        // diagonal slot knocked out of it, running from near the middle
        // down to the lower-left rim. Read as a ring first, and drawn
        // that way it came out a blank blob — a ring's hole and a slot
        // are the same few dark pixels at this size, and only where they
        // *stop* tells them apart.
        //
        // The slot measures as a round-capped stroke rather than the
        // arrow it looks like: its width runs 1.9, 3.5, 3.6, 3.3, 1.9
        // down its length, widest in the middle. An arrowhead would put
        // the widest part at the tip. So it is drawn as the stadium a
        // 2.9-wide round-capped line sweeps, spelled out as arcs because
        // the shape has to be knocked out of the disc by even-odd rather
        // than painted over it — the plate behind is transparent in the
        // resting state, so there is no colour to paint the notch in.
        EnvcpGlyph::Learn => {
            "M 10.1 11 A 4.9 4.9 0 1 0 19.9 11 A 4.9 4.9 0 1 0 10.1 11 Z \
             M 13.729 9.745 A 1.45 1.45 0 0 1 16.071 11.455 \
             L 13.371 15.155 A 1.45 1.45 0 0 1 11.029 13.445 Z"
        }
        // One period, ending short of the extremes at both ends: it
        // leaves (8.1, 12.6) climbing, peaks at (12.0, 7.0), crosses at
        // (14.7, 11.0), troughs at (17.9, 14.8) and stops at (21.9, 9.4)
        // still climbing. Read off the three near-vertical strands the
        // middle rows show, where the curve's centre is legible to a
        // tenth of a pixel, and the horizontal runs at the peak and
        // trough. The control points are then solved so each segment's
        // midpoint lands on the measured extreme rather than eyeballed —
        // eyeballing put the peak a pixel and a bit left and cost more
        // than every other error in this glyph combined.
        EnvcpGlyph::ParamMod => {
            "M 8.1 12.6 C 10.4 5.4 14 5.4 14.7 11 C 15.4 16.6 20.13 16.07 21.9 9.4"
        }
    };

    rsx! {
        svg {
            width: "{props.width.unwrap_or(vw as u32)}",
            height: "{props.height.unwrap_or(vh as u32)}",
            view_box: "0 0 {vw} {vh}",
            xmlns: "http://www.w3.org/2000/svg",
            rect {
                x: "0.5", y: "0.5", width: "{vw - 1.0}", height: "{vh - 1.0}",
                rx: "4.5",
                fill: "{plate_fill}",
                stroke: "{edge.css()}", stroke_width: "1",
            }
            // The tab: two rows tall, and a pixel narrower at its foot
            // than at its head. Drawn as the trapezoid it measures as
            // rather than the arc it probably wants to be — at two pixels
            // the difference is under a tenth of one.
            path {
                d: "M 8 1 H 22 L 20 3 H 10 Z",
                fill: "{tab.css()}",
            }
            path {
                d: "{path}",
                fill: "{glyph_fill}",
                fill_rule: "evenodd",
                stroke: "{ink.css()}",
                stroke_width: "{glyph_stroke}",
                stroke_linecap: "round",
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct EnvcpOptionsProps {
    #[props(default = (36.0, 20.0))]
    pub cell: (f32, f32),
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    #[props(default)]
    pub at: Interaction,
}

/// The envelope panel's options button — a gear and a drop caret.
///
/// Its plate is not the outlined one the other two use: it is a black
/// scrim at a fifth opacity with a slightly heavier rim, so it darkens
/// whatever is behind it instead of covering it.
#[component]
pub fn EnvcpOptionsButton(props: EnvcpOptionsProps) -> Element {
    let t = Theme::default();
    let (vw, vh) = props.cell;
    // The source prints `#b7b7b7` at alpha 0.79 — *instead of* the scrim,
    // not on top of it. Painted over a fifth of black the same values
    // composite to `#a5a5a5` at 0.84, which is six levels dark across the
    // whole mark and was most of this image's error. So the paint is
    // pre-compensated: lighter and less opaque, chosen to land on the
    // source's composite rather than to equal its nominal colour.
    let ink = match props.at {
        Interaction::Hover => t.chrome.hardware_mark.shade(0.753),
        _ => t.chrome.hardware_mark.shade(0.366),
    };
    // A *six*-tooth gear, which is the whole finding here. Drawn with
    // eight it painted a tooth at three and nine o'clock — where the
    // source's widest rows are 4.7 from the middle, a valley — and left
    // the real teeth at roughly ±68° unpainted. The difference is 144
    // levels either side and reads, at a glance, as the gear simply being
    // wrong.
    //
    // The rest is measured: teeth reach exactly 6.0 (the mark runs y 4..16
    // and x 6..18 about (12, 10)), the root sits at 4.7, and the hub is
    // 2.05 — nearly half the root, much wider than a gear icon usually
    // draws it.
    let cx = 12.0f32;
    let cy = 10.0f32;
    let teeth: Vec<(f32, f32, f32)> = (0..6)
        .map(|i| {
            let a = std::f32::consts::PI / 3.0 * i as f32;
            (cx + 4.68 * a.sin(), cy - 4.68 * a.cos(), a.to_degrees())
        })
        .collect();

    rsx! {
        svg {
            width: "{props.width.unwrap_or(vw as u32)}",
            height: "{props.height.unwrap_or(vh as u32)}",
            view_box: "0 0 {vw} {vh}",
            xmlns: "http://www.w3.org/2000/svg",
            // The scrim fills the cell edge to edge — it does not sit
            // inset by half a pixel, which is what a single stroked rect
            // gives you and what made the whole perimeter a shade light.
            //
            // Its rim is barely a rim, and it *fades*: 0.251 at the
            // outermost row, 0.224 at the next, 0.20 from there in. Alpha
            // falling inward cannot be stacked layers of black — more
            // black only ever adds — so it is the base plus two hairlines
            // carrying the difference.
            rect {
                x: "0", y: "0", width: "{vw}", height: "{vh}", rx: "2.8",
                fill: "#000000", fill_opacity: "0.20",
            }
            rect {
                x: "0.5", y: "0.5", width: "{vw - 1.0}", height: "{vh - 1.0}",
                rx: "2.4", fill: "none",
                stroke: "#000000", stroke_opacity: "0.064", stroke_width: "1",
            }
            rect {
                x: "1.5", y: "1.5", width: "{vw - 3.0}", height: "{vh - 3.0}",
                rx: "1.8", fill: "none",
                stroke: "#000000", stroke_opacity: "0.030", stroke_width: "1",
            }
            g { fill: "{ink.css()}", fill_opacity: "0.74",
                for (i, (tx, ty, deg)) in teeth.iter().enumerate() {
                    rect {
                        key: "t{i}",
                        x: "{tx - 1.575}", y: "{ty - 1.4}",
                        width: "3.15", height: "2.8", rx: "0.4",
                        transform: "rotate({deg} {tx} {ty})",
                    }
                }
                path { d: "{ring(cx, cy, 4.35, 2.05)}", fill_rule: "evenodd" }
                // The caret, which is a separate mark: it says the button
                // opens a menu, and it sits clear of the gear's teeth.
                //
                // Its rows measure 5.54, 4.34, 3.36, 1.98 and 0.64 wide,
                // and those are widths at each row's *middle* — taking
                // the first for the width at y=8 drew the whole triangle
                // half a row small.
                path { d: "M 23.42 8 H 29.58 L 26.5 13.02 Z" }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct EnvcpArmProps {
    /// Armed — the ring goes accent and the body lifts off black.
    #[props(default)]
    pub armed: bool,
    #[props(default = (20.0, 20.0))]
    pub cell: (f32, f32),
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    #[props(default)]
    pub at: Interaction,
}

/// The envelope arm button — a ring on a disc.
///
/// Geometrically the mixer's record button without its housing: body
/// r 9.90 about the cell's centre, ring 4.90 outside and 2.68 in. Unarmed
/// it is flat `#1c1c1c` throughout; armed the body picks up a radial
/// gradient lit from above the top edge, which is the only place in this
/// family a gradient appears at all.
#[component]
pub fn EnvcpArmButton(props: EnvcpArmProps) -> Element {
    let t = Theme::default();
    let (vw, vh) = props.cell;
    let (cx, cy) = (vw * 0.5, vh * 0.5);
    let unit = vw.min(vh);
    let hovered = props.at == Interaction::Hover;

    let ink = match (props.armed, hovered) {
        (true, false) => t.chrome.accent,
        // Not the accent offset or scaled: the source's hover runs red up
        // 58 levels, green up 21 and blue *down* 3, which is a wash
        // toward white in one channel only. Carried as measured.
        (true, true) => Color::rgb(0x80, 0xce, 0xfb),
        (false, false) => t.chrome.hardware.shade(0.214),
        (false, true) => t.chrome.hardware.shade(0.406),
    };
    let bump = if hovered { 4.0 } else { 0.0 };
    // Unarmed the body is one flat `#1c1c1c`; armed it takes a radial
    // gradient lit from above the top edge — `#252525` under the light,
    // `#1f1f1f` two-thirds out, `#181818` at the rim. That gradient is
    // the only one in this family, and it is genuinely radial: the
    // brightness falls with distance from a point near (10, 2), not with
    // height, which a vertical ramp cannot reproduce at the corners.
    let flat = offset(t.chrome.hardware_edge.shade(0.0216), bump);
    let glow = offset(t.chrome.hardware_edge.shade(0.0603), bump);
    let body = offset(t.chrome.hardware_edge.shade(0.0345), bump);
    let sink = offset(t.chrome.hardware_edge.shade(0.0043), bump);
    // The glow moves with hover (`bump`), so a hovered armed button beside
    // a resting one is two gradients.
    let arm_id = vary("envarm", &[&glow.css(), &body.css(), &sink.css()]);
    let body_fill = if props.armed {
        format!("url(#{arm_id})")
    } else {
        flat.css()
    };

    rsx! {
        svg {
            width: "{props.width.unwrap_or(vw as u32)}",
            height: "{props.height.unwrap_or(vh as u32)}",
            view_box: "0 0 {vw} {vh}",
            xmlns: "http://www.w3.org/2000/svg",
            defs {
                radialGradient {
                    id: "{arm_id}",
                    cx: "{cx}", cy: "{vh * 0.10}", r: "{unit * 0.85}",
                    gradient_units: "userSpaceOnUse",
                    stop { offset: "0", stop_color: "{glow.css()}" }
                    stop { offset: "0.70", stop_color: "{body.css()}" }
                    stop { offset: "1", stop_color: "{sink.css()}" }
                }
            }
            circle {
                cx: "{cx}", cy: "{cy}", r: "{unit * 0.495}",
                fill: "{body_fill}",
            }
            path {
                d: "{ring(cx, cy, unit * 0.245, unit * 0.134)}",
                fill_rule: "evenodd",
                fill: "{ink.css()}",
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct EnvcpBypassProps {
    /// Bypassed — the whole button turns red.
    #[props(default)]
    pub bypassed: bool,
    #[props(default = (15.0, 20.0))]
    pub cell: (f32, f32),
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    #[props(default)]
    pub at: Interaction,
}

/// The envelope bypass button — a power symbol on a tinted field.
///
/// The field is not a shade of the glyph: solved as `base + t·glyph` the
/// two states want different bases (`#0d0d0d` at t 0.17 for the blue,
/// `#1c1c1c` at t 0.14 for the red), so they are carried as the two
/// colours they measure as rather than as one rule bent to fit both.
#[component]
pub fn EnvcpBypassButton(props: EnvcpBypassProps) -> Element {
    let t = Theme::default();
    let (vw, vh) = props.cell;
    let hovered = props.at == Interaction::Hover;

    let ink = match (props.bypassed, hovered) {
        (false, false) => Color::rgb(0x68, 0xb7, 0xf8),
        (false, true) => Color::rgb(0x7c, 0xcc, 0xfc),
        (true, false) => Color::rgb(0xff, 0x52, 0x60),
        (true, true) => Color::rgb(0xff, 0x63, 0x73),
    };
    let field = match (props.bypassed, hovered) {
        (false, false) => Color::rgb(0x1b, 0x2a, 0x35),
        (false, true) => Color::rgb(0x22, 0x34, 0x41),
        (true, false) => Color::rgb(0x3b, 0x24, 0x26),
        (true, true) => Color::rgb(0x49, 0x2d, 0x2f),
    };
    let _ = &t;

    rsx! {
        svg {
            width: "{props.width.unwrap_or(vw as u32)}",
            height: "{props.height.unwrap_or(vh as u32)}",
            view_box: "0 0 {vw} {vh}",
            xmlns: "http://www.w3.org/2000/svg",
            rect { x: "0", y: "0", width: "{vw}", height: "{vh}", fill: "{field.css()}" }
            // The arc opens 50° either side of vertical, which is where
            // the stem passes through it.
            path {
                d: "M 4.658 8.128 A 3.71 3.71 0 1 0 10.342 8.128",
                fill: "none",
                stroke: "{ink.css()}", stroke_width: "1.44",
            }
            rect { x: "7", y: "5.25", width: "1", height: "4.5", fill: "{ink.css()}" }
        }
    }
}

// ── meters ──────────────────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
pub struct MeterProps {
    /// One value per bar, 0 at silence and 1 at the top of the scale.
    pub levels: Vec<f32>,
    /// Decaying peak-hold, one value per bar in the same units as
    /// `levels`. The feed computes these publisher-side; a bar with no
    /// entry — or a hold at silence — draws no mark.
    #[props(default)]
    pub holds: Vec<f32>,
    /// Cell size. `mcp.meter` is `[4 4 22 -4]` of the stretch section and
    /// two wider again in wide mode, so 24 by whatever is left.
    pub cell: (f32, f32),
    /// Print the dB scale down the left of the meter.
    ///
    /// It belongs *inside* the meter and not beside it: `mcp.meter` is one
    /// rect covering both, and REAPER draws the numbers as part of the
    /// widget. Laid out as a separate column the bars had nowhere to go
    /// but under the button stack.
    #[props(default = true)]
    pub scale: bool,
    /// The colour the unlit bars sit on. `None` is the mixer's dark
    /// trough; the track panel passes its own background so the meter is
    /// invisible at rest, which is what REAPER's is.
    #[props(default)]
    pub well: Option<String>,
    // No `tag` prop any more: it guarded per-instance clip regions that
    // have since been refactored away, and the surviving `mtr` gradient is
    // built only from the theme, so every instance's copy is identical.
    // The rule it embodied — ids need distinguishing exactly when their
    // content varies per instance — moved to the module docs, and the
    // mechanism to [`vary`].
    /// The scale's marks, top to bottom.
    #[props(default)]
    pub marks: Vec<String>,
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    #[props(default)]
    pub at: Interaction,
}

/// A level meter — bars, and the scale that belongs to them.
///
/// The one control here REAPER draws entirely itself: there is no
/// `mcp_meter*` bitmap to measure, only `mcp.meter.scale.color.*` and the
/// VU division in `rtconfig`. So this is the theme's own reading of it
/// rather than a trace, and the two places it departs from REAPER are
/// deliberate: the bars sit on a dark well, which REAPER leaves bare, and
/// the ladder runs green to amber to red rather than one colour lit and
/// unlit.
#[component]
pub fn Meter(props: MeterProps) -> Element {
    let t = Theme::default();
    let (vw, vh) = props.cell;
    // The scale takes four-fifths of the block and the bars share the
    // rest, which is why REAPER's meters are as thin as they are: "-18-"
    // at nine pixels is most of a 24-wide `mcp.meter`. At two-thirds the
    // marks ran off their own left edge and rendered as "18-".
    // 0.85, measured: in a 26-wide block at x=4, REAPER's scale runs 7..24
    // and its fader cap starts at 30 — so the bars get the five columns
    // between them and the scale gets everything to their left. At 0.80 the
    // whole scale sat three columns left of REAPER's.
    // The bars are the *whole* block, scale included. REAPER's meter is one
    // wide rectangle with the numbers printed inside it — the green fills
    // the lot and the marks sit on top — not a pair of hairlines beside a
    // column of type, which is what reserving most of the width for the
    // scale produced.
    let bars_x = 0.0f32;
    let bars_w = vw;
    let n = props.levels.len().max(1) as f32;
    let gap = 1.0f32;
    let bar_w = ((bars_w - gap * (n - 1.0)) / n).max(1.0);
    let _text = t.chrome.hardware_mark.shade(-0.35);
    // The well the bars sit in. The mixer's is a dark trough — a
    // deliberate departure, REAPER leaves its bare — but the track panel's
    // meter is a bare strip against the panel and a trough there is a pair
    // of black bars where REAPER has nothing at rest.
    let well = props
        .well
        .clone()
        .unwrap_or_else(|| t.chrome.hardware.shade(-0.72).css());

    rsx! {
        svg {
            width: "{props.width.unwrap_or(vw as u32)}",
            height: "{props.height.unwrap_or(vh as u32)}",
            view_box: "0 0 {vw} {vh}",
            xmlns: "http://www.w3.org/2000/svg",
            defs {
                // Green until it is loud, amber approaching the top, red
                // at it — read bottom-up, so the stops are reversed.
                linearGradient { id: "mtr", x1: "0", y1: "1", x2: "0", y2: "0",
                    stop { offset: "0", stop_color: "{t.signal.meter_safe.css()}" }
                    stop { offset: "0.62", stop_color: "{t.signal.meter_safe.css()}" }
                    stop { offset: "0.82", stop_color: "{t.signal.meter_warn.css()}" }
                    stop { offset: "1", stop_color: "{t.signal.meter_danger.css()}" }
                }
            }
            for (i, level) in props.levels.iter().enumerate() {
                {
                    let x = bars_x + i as f32 * (bar_w + gap);
                    let lit = vh * level.clamp(0.0, 1.0);
                    let hold = props.holds.get(i).copied().unwrap_or(0.0).clamp(0.0, 1.0);
                    rsx! {
                        g { key: "b{i}",
                            rect {
                                x: "{x}", y: "0", width: "{bar_w}", height: "{vh}",
                                rx: "{bar_w * 0.25}",
                                fill: "{well}",
                            }
                            rect {
                                x: "{x}", y: "{vh - lit}", width: "{bar_w}", height: "{lit}",
                                rx: "{bar_w * 0.25}",
                                fill: "url(#mtr)",
                            }
                            // The peak-hold mark — one row, held where the
                            // peak was while the bar falls away under it.
                            if hold > 0.0 {
                                rect {
                                    x: "{x}", y: "{(vh - vh * hold).min(vh - 1.0)}",
                                    width: "{bar_w}", height: "1",
                                    fill: "#ffffff", fill_opacity: "0.7",
                                }
                            }
                        }
                    }
                }
            }
            if props.scale {
                {
                    // Printed *over* the bars: the meter is one block with
                    // its numbers inside it.
                    //
                    // The first mark is the peak *readout*, not a scale
                    // mark. REAPER prints it at the top of the meter and
                    // spaces the dB marks evenly through what is left —
                    // measured, its marks step 21.75 in a 118-row meter
                    // whose readout sits at 10. Spacing all six evenly
                    // instead put every middle mark eight rows low.
                    // Two separate anchors, because they are two separate
                    // things: the readout sits at 0.055 of the meter (6.5
                    // rows of REAPER's 119) and the marks below it start at
                    // 0.0914, which is where its 21.75 step lands them.
                    let head = vh * 0.055;
                    let top = vh * 0.0914;
                    let rest = (props.marks.len().max(2) - 1) as f32;
                    // 0.43 and 11.2, measured: REAPER's "-18-" is 17
                    // columns wide and eight rows tall, ours was 13 by six.
                    // The marks are the biggest thing in the meter's block
                    // and reading them at a glance is the whole point of a
                    // scale, so the type is REAPER's size rather than a
                    // size that merely fits.
                    let size = (vw * 0.43).min(11.2);
                    // Anchored on the bars, with no gap of its own: the
                    // glyphs' own right side bearing is the gap, and the
                    // two columns this used to reserve were two columns
                    // REAPER spends on the scale.
                    // The block's right edge, less a column. The marks were
                    // anchored on `bars_x`, which was the bars' left edge
                    // back when the scale had its own column beside them —
                    // now that the bars *are* the block, that is 0 and the
                    // whole scale hung off the left of the meter.
                    let x = vw - 1.0;
                    // White at 0.55 — REAPER's unlit scale colour, `[255
                    // 255 255 140]`. It prints black over the lit part too
                    // (`[0 0 0 170]`); that wants two clipped passes, and
                    // the clip cut the glyphs rather than the bars in this
                    // renderer. One legible pass beats two broken ones.
                    let ink = "#ffffff";
                    rsx! {
                        for (i, mark) in props.marks.iter().enumerate() {
                            {
                                let y = if i == 0 {
                                    head
                                } else {
                                    top + (i as f32 - 0.5) * (vh - top) / rest
                                };
                                rsx! {
                                    text {
                                        key: "m{i}",
                                        x: "{x}", y: "{y}",
                                        text_anchor: "end",
                                        dominant_baseline: "central",
                                        font_family: "Fira Sans, DejaVu Sans, sans-serif",
                                        font_size: "{size}",
                                        fill: "{ink}",
                                        fill_opacity: "0.55",
                                        "{mark}"
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

/// A piece of the envelope panel's furniture.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum EnvcpPart {
    /// The fader's track — a slab with a notch at its middle.
    #[default]
    FaderTrack,
    /// The field behind the arm button: a half-disc, flat on its right.
    ArmField,
    /// The envelope knob's body.
    Knob,
    /// The fader's cap — the one piece here with a drop shadow.
    FaderCap,
}

#[derive(Props, Clone, PartialEq)]
pub struct EnvcpPanelProps {
    #[props(default)]
    pub part: EnvcpPart,
    pub cell: (f32, f32),
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    #[props(default)]
    pub at: Interaction,
}

/// The envelope panel's furniture — track, knob, cap and arm field.
///
/// Three of these round only their *right* corners and leave the left
/// square, which is not a style choice: they butt against the panel's
/// left edge, so REAPER never shows the corner that would be square.
#[component]
pub fn EnvcpPanel(props: EnvcpPanelProps) -> Element {
    let t = Theme::default();
    let (vw, vh) = props.cell;
    let g = |v: f32| t.chrome.hardware.shade(v);
    let slab = g(-0.56); // #1c1c1c

    // The cap's body is a cylinder lit from its left of centre: the
    // colour ramps #222222 up to #797979 at x9, hits an opaque
    // grey/blue/grey triple at the middle, then falls back to #222222.
    // Its *alpha* ramps too, 0.745 to 0.816, independently of the
    // colour — so the stops carry both.
    let cap: Vec<(f32, &str, f32)> = vec![
        (0.000, "#222222", 0.745),
        (0.029, "#222222", 0.745),
        (0.088, "#3f3f3f", 0.757),
        (0.147, "#464646", 0.765),
        (0.206, "#4a4a4a", 0.749),
        (0.265, "#575757", 0.757),
        (0.324, "#656565", 0.769),
        (0.382, "#797979", 0.804),
        (0.618, "#626262", 0.773),
        (0.676, "#5c5c5c", 0.757),
        (0.735, "#5b5b5b", 0.765),
        (0.794, "#585858", 0.773),
        (0.853, "#595959", 0.804),
        (0.912, "#545454", 0.816),
        (0.971, "#222222", 0.745),
        (1.000, "#222222", 0.745),
    ];

    rsx! {
        svg {
            width: "{props.width.unwrap_or(vw as u32)}",
            height: "{props.height.unwrap_or(vh as u32)}",
            view_box: "0 0 {vw} {vh}",
            xmlns: "http://www.w3.org/2000/svg",
            defs {
                linearGradient { id: "envcap", x1: "0", y1: "0", x2: "1", y2: "0",
                    for (i, (at, hex, a)) in cap.iter().enumerate() {
                        stop {
                            key: "c{i}", offset: "{at}",
                            stop_color: "{hex}", stop_opacity: "{a}",
                        }
                    }
                }
                // The knob's face falls fast for its first three rows and
                // then slowly: 97 at the top, 81 three rows down, 63 at
                // the bottom. One straight ramp between the ends misses
                // the highlight by fifteen levels.
                linearGradient { id: "envknob", x1: "0", y1: "0", x2: "0", y2: "1",
                    stop { offset: "0", stop_color: "{g(0.208).css()}" }
                    stop { offset: "0.11", stop_color: "{g(0.177).css()}" }
                    stop { offset: "0.26", stop_color: "{g(0.094).css()}" }
                    stop { offset: "1", stop_color: "{g(-0.032).css()}" }
                }
                radialGradient { id: "envshadow",
                    cx: "11.5", cy: "20.5", r: "9.5",
                    gradient_units: "userSpaceOnUse",
                    stop { offset: "0.45", stop_color: "#000000", stop_opacity: "0.30" }
                    stop { offset: "1", stop_color: "#000000", stop_opacity: "0" }
                }
                linearGradient { id: "envcapgloss", x1: "0", y1: "0", x2: "0", y2: "1",
                    stop { offset: "0", stop_color: "#ffffff", stop_opacity: "0.13" }
                    stop { offset: "1", stop_color: "#ffffff", stop_opacity: "0" }
                }
            }
            match props.part {
                EnvcpPart::FaderTrack => rsx! {
                    // Square on the left, rounded on the right, with a
                    // hard black notch at the middle row.
                    path {
                        d: "M 1 2 H 15.5 A 2.5 2.5 0 0 1 18 4.5 V 19.5
                            A 2.5 2.5 0 0 1 15.5 22 H 1 Z",
                        fill: "{slab.css()}",
                    }
                    rect { x: "6", y: "11", width: "7", height: "1", fill: "#000000" }
                },
                EnvcpPart::ArmField => rsx! {
                    // A disc of r 10 about (11, 11) with a square block
                    // filling out its right half — *not* a semicircle
                    // whose chord is the right edge. Written that way the
                    // two endpoints sat on x 21, which puts the circle's
                    // centre there too and bulges the arc off the canvas.
                    path {
                        d: "M 11 1 A 10 10 0 0 0 11 21 H 21 V 1 Z",
                        fill: "{slab.css()}",
                    }
                },
                EnvcpPart::Knob => rsx! {
                    // The slab behind the knob is not a rounded rect: its
                    // right edge is an arc concentric with the knob, 0.9
                    // outside the rim. It reaches x 23.9 at the middle
                    // row and x 17.7 at the top, which no corner radius
                    // does — same rect-plus-disc as the arm field.
                    rect { x: "1", y: "2", width: "12.5", height: "20",
                        fill: "{slab.css()}" }
                    circle { cx: "13.5", cy: "12", r: "10.25", fill: "{slab.css()}" }
                    circle { cx: "13.5", cy: "12", r: "9.5", fill: "{g(-0.60).css()}" }
                    circle { cx: "13.5", cy: "12", r: "8.2", fill: "url(#envknob)" }
                },
                EnvcpPart::FaderCap => rsx! {
                    // The shadow goes first and the cap covers all of it
                    // that is not below the cap — which is how the source
                    // has it, and cheaper than a blur resvg may not run.
                    ellipse { cx: "11.5", cy: "20.5", rx: "9.2", ry: "6.8",
                        fill: "url(#envshadow)" }
                    rect { x: "2", y: "10", width: "1", height: "12",
                        fill: "#000000", fill_opacity: "0.125" }
                    rect { x: "20", y: "10", width: "1", height: "12",
                        fill: "#000000", fill_opacity: "0.125" }
                    // Top and bottom row are rims, not body: the source's
                    // row 5 is a flat `#262626` at 0.66 and its row 21 a
                    // flat `#222222` at 0.745, both of them a good sixty
                    // levels below the cylinder they cap.
                    rect { x: "3", y: "5", width: "17", height: "1", rx: "1",
                        fill: "#262626", fill_opacity: "0.66" }
                    rect {
                        x: "3", y: "6", width: "17", height: "15",
                        fill: "url(#envcap)",
                    }
                    rect { x: "3", y: "21", width: "17", height: "1", rx: "1",
                        fill: "#222222", fill_opacity: "0.745" }
                    rect { x: "3", y: "6", width: "17", height: "2.5",
                        fill: "url(#envcapgloss)" }
                    rect { x: "10", y: "6", width: "1", height: "15",
                        fill: "{g(0.10).css()}" }
                    rect { x: "12", y: "6", width: "1", height: "15",
                        fill: "{g(0.10).css()}" }
                    // The one saturated mark on the whole panel.
                    rect { x: "11", y: "6", width: "1", height: "15",
                        fill: "#16a9fe" }
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::render_svg;

    fn valid(svg: &str) -> bool {
        let opts = resvg::usvg::Options::default();
        resvg::usvg::Tree::from_str(svg, &opts).is_ok()
    }

    /// The SVG id rule, pinned: a def whose content varies per instance
    /// must carry a different id per content. Two FX pills in different
    /// states are the canonical pair — before [`vary`], both said
    /// `id="fxface"` and the first one in the document painted them both.
    #[test]
    fn defs_that_vary_per_instance_do_not_share_ids() {
        let pill = |at| {
            render_svg(
                FxButton,
                FxProps {
                    family: Default::default(),
                    state: FxChain::Active,
                    width: None,
                    height: None,
                    at,
                },
            )
        };
        // Hover is what moves the *plate* — chain state only moves the ink.
        let resting = pill(Interaction::Normal);
        let active = pill(Interaction::Hover);

        let id_of = |svg: &str| {
            let at = svg.find("id=\"fxface").expect("a derived fxface id");
            svg[at..].split('"').nth(1).unwrap().to_string()
        };
        assert_ne!(
            id_of(&resting),
            id_of(&active),
            "two states share one gradient id — the first in the document \
             would paint both"
        );
    }

    /// A control's own size, as declared by the SVG it renders.
    fn intrinsic(svg: &str) -> (f32, f32) {
        let opts = resvg::usvg::Options::default();
        let tree = resvg::usvg::Tree::from_str(svg, &opts).expect("valid svg");
        (tree.size().width(), tree.size().height())
    }

    /// Every control draws at the aspect of the art it replaces.
    ///
    /// FX shared mute's 21x20 for a while, when `mcp_fx_*` cells are 28x22.
    /// Nothing failed: the button simply got stretched into its cell by
    /// whatever drew it, which looks like a rendering bug and is in fact a
    /// wrong `viewBox`. The cell sizes come from the compiled-in art index,
    /// so this checks against the real images rather than repeating numbers
    /// that could go stale in both places at once.
    #[test]
    fn every_control_is_shaped_like_the_cell_it_replaces() {
        let n = (None, None);
        let cases: [(&str, String); 7] = [
            (
                "mcp_recarm_on",
                render_svg(
                    RecordArmButton,
                    RecordArmProps {
                        art: crate::slice::expect_art("mcp_recarm_on"),
                        housing: true,
                        state: RecordArm::On,
                        width: n.0,
                        height: n.1,
                        at: Interaction::Normal,
                    },
                ),
            ),
            (
                "mcp_mute_on",
                render_svg(
                    MuteButton,
                    ToggleProps {
                        unlit: None,
                        hover: 0.35,
                        sinks: true,
                        depth: 0.15,
                        legend: None,
                        art: crate::slice::expect_art("mcp_mute_on"),
                        body: (0.0, 1.0),
                        on: true,
                        width: n.0,
                        height: n.1,
                        at: Interaction::Normal,
                    },
                ),
            ),
            (
                "mcp_solo_on",
                render_svg(
                    SoloButton,
                    SoloProps {
                        unlit: None,
                        hover: 0.35,
                        sinks: true,
                        depth: 0.11,
                        legend: None,
                        art: crate::slice::expect_art("mcp_solo_on"),
                        body: (0.0, 1.0),
                        state: Solo::On,
                        width: n.0,
                        height: n.1,
                        at: Interaction::Normal,
                    },
                ),
            ),
            (
                "mcp_fx_norm",
                render_svg(
                    FxButton,
                    FxProps {
                        family: Default::default(),
                        state: FxChain::Active,
                        width: n.0,
                        height: n.1,
                        at: Interaction::Normal,
                    },
                ),
            ),
            (
                "mcp_io_s_r",
                render_svg(
                    RoutingButton,
                    RoutingProps {
                        art: crate::slice::expect_art("mcp_io_s_r"),
                        axis: Default::default(),
                        has_sends: true,
                        has_receives: true,
                        disabled: false,
                        width: n.0,
                        height: n.1,
                        at: Interaction::Normal,
                    },
                ),
            ),
            (
                "mcp_monitor_on",
                render_svg(
                    InputMonitorIndicator,
                    MonitoringProps {
                        art: crate::slice::expect_art("mcp_monitor_on"),
                        axis: Default::default(),
                        state: Monitoring::On,
                        width: n.0,
                        height: n.1,
                        at: Interaction::Normal,
                    },
                ),
            ),
            (
                "mcp_volthumb",
                render_svg(
                    VolumeFaderCap,
                    FaderCapProps {
                        accent: None,
                        pane: None,
                        width: n.0,
                        height: n.1,
                    },
                ),
            ),
        ];

        for (name, svg) in &cases {
            let art = crate::generated::by_name(name)
                .unwrap_or_else(|| panic!("no art index entry for {name}"));
            let cell_w = art.width as f32 / art.cells.max(1) as f32;
            let cell_h = art.height as f32;
            let (vw, vh) = intrinsic(svg);

            // Cell widths are not always whole (86/3), so compare aspect
            // with a tolerance rather than demanding exact pixels.
            let want = cell_w / cell_h;
            let got = vw / vh;
            assert!(
                (want - got).abs() < 0.06,
                "{name}: cell is {cell_w}x{cell_h} (aspect {want:.3}) \
                 but the vector is {vw}x{vh} (aspect {got:.3})",
            );
        }
    }

    #[test]
    fn every_control_produces_valid_svg_at_any_size() {
        // Proportional geometry can still go negative if a fraction is
        // wrong; resvg rejects that, and only at render time.
        for (w, h) in [(8, 8), (20, 20), (200, 200), (1200, 1200)] {
            let (w, h) = (Some(w), Some(h));
            let cases = [
                render_svg(
                    MuteButton,
                    ToggleProps {
                        unlit: None,
                        hover: 0.35,
                        sinks: true,
                        depth: 0.15,
                        legend: None,
                        body: (0.0, 1.0),
                        art: crate::slice::expect_art("mcp_mute_on"),
                        on: true,
                        width: w,
                        height: h,
                        at: Interaction::Normal,
                    },
                ),
                render_svg(
                    SoloButton,
                    SoloProps {
                        unlit: None,
                        hover: 0.35,
                        sinks: true,
                        depth: 0.11,
                        legend: None,
                        body: (0.0, 1.0),
                        art: crate::slice::expect_art("mcp_solo_on"),
                        state: Solo::Defeat,
                        width: w,
                        height: h,
                        at: Interaction::Normal,
                    },
                ),
                render_svg(
                    FxButton,
                    FxProps {
                        family: Default::default(),
                        state: FxChain::Active,
                        width: w,
                        height: h,
                        at: Interaction::Normal,
                    },
                ),
                render_svg(
                    RecordArmButton,
                    RecordArmProps {
                        art: crate::slice::expect_art("mcp_recarm_on"),
                        housing: true,
                        state: RecordArm::NoRecord,
                        width: w,
                        height: h,
                        at: Interaction::Normal,
                    },
                ),
                render_svg(
                    RoutingButton,
                    RoutingProps {
                        art: crate::slice::expect_art("mcp_io_s_r"),
                        axis: Default::default(),
                        has_sends: true,
                        has_receives: true,
                        disabled: false,
                        width: w,
                        height: h,
                        at: Interaction::Normal,
                    },
                ),
                render_svg(
                    InputMonitorIndicator,
                    MonitoringProps {
                        art: crate::slice::expect_art("mcp_monitor_on"),
                        axis: Default::default(),
                        state: Monitoring::On,
                        width: w,
                        height: h,
                        at: Interaction::Normal,
                    },
                ),
                render_svg(
                    PanningKnob,
                    PanProps {
                        indicator: false,
                        position: -0.5,
                        large: true,
                        width: w,
                        height: h,
                    },
                ),
                render_svg(
                    VolumeFaderCap,
                    FaderCapProps {
                        accent: None,
                        pane: None,
                        width: w,
                        height: h,
                    },
                ),
                render_svg(
                    VolumeFaderTrack,
                    FaderCapProps {
                        accent: None,
                        pane: None,
                        width: w,
                        height: h,
                    },
                ),
            ];
            for (i, svg) in cases.iter().enumerate() {
                assert!(valid(svg), "control {i} invalid at {w:?}x{h:?}: {svg}");
            }
        }
    }

    #[test]
    fn nothing_is_drawn_with_a_pixel_constant() {
        // A hardcoded stroke or radius is what stops a vector scaling: it
        // looks right at the size it was written for and wrong everywhere
        // else. Every geometry attribute must be a fraction of the viewBox,
        // so scaling the box must scale the drawing.
        let small = render_svg(
            MuteButton,
            ToggleProps {
                unlit: None,
                hover: 0.35,
                sinks: true,
                depth: 0.15,
                legend: None,
                body: (0.0, 1.0),
                art: crate::slice::expect_art("mcp_mute_on"),
                on: false,
                width: Some(21),
                height: Some(20),
                at: Interaction::Normal,
            },
        );
        let large = render_svg(
            MuteButton,
            ToggleProps {
                unlit: None,
                hover: 0.35,
                sinks: true,
                depth: 0.15,
                legend: None,
                body: (0.0, 1.0),
                art: crate::slice::expect_art("mcp_mute_on"),
                on: false,
                width: Some(210),
                height: Some(200),
                at: Interaction::Normal,
            },
        );
        // Same geometry, different render size: the viewBox does the work.
        let strip = |s: &str| {
            s.split("width=\"")
                .nth(1)
                .map(|r| r.split('"').next().unwrap().to_string())
        };
        assert_ne!(strip(&small), strip(&large));
        // And the internal coordinates are identical, i.e. resolution-free.
        let body = |s: &str| s.split("<rect").nth(1).unwrap_or("").to_string();
        assert_eq!(body(&small), body(&large));
    }

    #[test]
    fn the_pan_knob_rotates_continuously() {
        // The traced version picks one of 128 baked frames; the whole point
        // of the vector one is that it does not have to.
        let a = render_svg(
            PanningKnob,
            PanProps {
                indicator: false,
                position: -1.0,
                large: false,
                width: None,
                height: None,
            },
        );
        let b = render_svg(
            PanningKnob,
            PanProps {
                indicator: false,
                position: -0.99,
                large: false,
                width: None,
                height: None,
            },
        );
        let c = render_svg(
            PanningKnob,
            PanProps {
                indicator: false,
                position: 1.0,
                large: false,
                width: None,
                height: None,
            },
        );
        assert_ne!(a, b, "a 1% pan change moved nothing");
        assert_ne!(a, c);
    }

    #[test]
    fn an_out_of_range_pan_clamps() {
        for p in [-9.0, 9.0, f32::NAN] {
            let svg = render_svg(
                PanningKnob,
                PanProps {
                    indicator: false,
                    position: p,
                    large: false,
                    width: None,
                    height: None,
                },
            );
            assert!(valid(&svg), "pan {p} produced invalid SVG");
        }
    }

    #[test]
    fn interaction_shifts_the_face() {
        // Hover and pressed are real states in the original art; a vector
        // control that ignores them loses feedback the theme had.
        let n = render_svg(
            MuteButton,
            ToggleProps {
                unlit: None,
                hover: 0.35,
                sinks: true,
                depth: 0.15,
                legend: None,
                body: (0.0, 1.0),
                art: crate::slice::expect_art("mcp_mute_on"),
                on: false,
                width: None,
                height: None,
                at: Interaction::Normal,
            },
        );
        let h = render_svg(
            MuteButton,
            ToggleProps {
                unlit: None,
                hover: 0.35,
                sinks: true,
                depth: 0.15,
                legend: None,
                body: (0.0, 1.0),
                art: crate::slice::expect_art("mcp_mute_on"),
                on: false,
                width: None,
                height: None,
                at: Interaction::Hover,
            },
        );
        let p = render_svg(
            MuteButton,
            ToggleProps {
                unlit: None,
                hover: 0.35,
                sinks: true,
                depth: 0.15,
                legend: None,
                body: (0.0, 1.0),
                art: crate::slice::expect_art("mcp_mute_on"),
                on: false,
                width: None,
                height: None,
                at: Interaction::Pressed,
            },
        );
        assert_ne!(n, h, "hover looks the same as normal");
        assert_ne!(n, p, "pressed looks the same as normal");
        assert_ne!(h, p);
    }

    #[test]
    fn states_stay_visually_distinct() {
        let off = render_svg(
            SoloButton,
            SoloProps {
                unlit: None,
                hover: 0.35,
                sinks: true,
                depth: 0.11,
                legend: None,
                body: (0.0, 1.0),
                art: crate::slice::expect_art("mcp_solo_on"),
                state: Solo::Off,
                width: None,
                height: None,
                at: Interaction::Normal,
            },
        );
        let on = render_svg(
            SoloButton,
            SoloProps {
                unlit: None,
                hover: 0.35,
                sinks: true,
                depth: 0.11,
                legend: None,
                body: (0.0, 1.0),
                art: crate::slice::expect_art("mcp_solo_on"),
                state: Solo::On,
                width: None,
                height: None,
                at: Interaction::Normal,
            },
        );
        let defeat = render_svg(
            SoloButton,
            SoloProps {
                unlit: None,
                hover: 0.35,
                sinks: true,
                depth: 0.11,
                legend: None,
                body: (0.0, 1.0),
                art: crate::slice::expect_art("mcp_solo_on"),
                state: Solo::Defeat,
                width: None,
                height: None,
                at: Interaction::Normal,
            },
        );
        assert_ne!(off, on);
        assert_ne!(on, defeat, "defeat must not look like plain solo");
    }

    #[test]
    fn the_fader_cap_keeps_its_ribs_at_every_size() {
        // The ribbed plastic look is the thing being preserved; deriving
        // the rib count from height would thin them out when zoomed.
        for h in [20u32, 53, 400] {
            let svg = render_svg(
                VolumeFaderCap,
                FaderCapProps {
                    accent: None,
                    pane: None,
                    width: None,
                    height: Some(h),
                },
            );
            // 7 ribs + body + panel + split.
            assert!(svg.matches("<rect").count() >= 10, "lost ribs at h={h}");
        }
    }

    #[test]
    fn the_cap_takes_a_track_accent() {
        let green = Color::rgb(0x3d, 0xdc, 0x97);
        let svg = render_svg(
            VolumeFaderCap,
            FaderCapProps {
                accent: Some(green),
                pane: None,
                width: None,
                height: None,
            },
        );
        // The grip is fourteen ribs of shaded variants, so the accent
        // never appears verbatim — asserting on its exact hex only passed
        // while the panel was a flat fill, and pinning any *particular*
        // shade just re-broke every time the ramp was retuned. What the
        // caller actually promises is that the accent reaches the grip
        // and nothing else, so check the hue instead of the value.
        // Counting rects was a proxy for "it has ribs" and broke the
        // moment the grip stopped being one rect per row — which was a
        // correction, not a regression. Only the hue is the promise.
        let greenish = svg
            .match_indices("fill=\"#")
            .filter_map(|(i, _)| svg.get(i + 7..i + 13))
            .filter_map(|h| Color::hex(&format!("#{h}")))
            .filter(|c| c.g > c.r && c.g > c.b)
            .count();
        assert!(greenish > 10, "the accent did not reach the grip");

        let plain = render_svg(
            VolumeFaderCap,
            FaderCapProps {
                accent: None,
                pane: None,
                width: None,
                height: None,
            },
        );
        assert_ne!(svg, plain, "the accent made no difference");
    }
}

#[cfg(test)]
mod rail_band_tests {
    use super::*;
    use crate::slice::Pane;

    fn render(pane: Pane, height: Option<u32>) -> String {
        let mut dom = dioxus::prelude::VirtualDom::new_with_props(
            VolumeFaderTrack,
            FaderCapProps {
                accent: None,
                pane: Some(pane),
                width: Some(23),
                height,
            },
        );
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    /// A growing band takes the layout's height, not the source band's.
    ///
    /// An `<svg>` is a replaced element: a `height` attribute beats the
    /// `flex:1` beside it, so a rail that stated 27 stayed 27 rows tall in
    /// a 300-row strip.
    #[test]
    fn a_growing_band_does_not_state_a_pixel_height() {
        let html = render(
            Pane {
                view: (0.0, 14.0, 23.0, 27.0),
                grow: true,
            },
            None,
        );
        assert!(
            html.contains("height=\"100%\""),
            "the growing band pinned a height:\n{html}"
        );
    }

    /// A fixed band still states one, and a caller that asks for a height
    /// still gets it — the exporter always asks.
    #[test]
    fn a_fixed_band_and_an_asked_for_height_are_unchanged() {
        let fixed = render(
            Pane {
                view: (0.0, 0.0, 23.0, 16.0),
                grow: false,
            },
            None,
        );
        assert!(
            fixed.contains("height=\"16\""),
            "the fixed band lost its height:\n{fixed}"
        );

        let asked = render(
            Pane {
                view: (0.0, 14.0, 23.0, 27.0),
                grow: true,
            },
            Some(55),
        );
        assert!(
            asked.contains("height=\"55\""),
            "an asked-for height was ignored:\n{asked}"
        );
    }
}

// ── track panel: the volume knob ────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
pub struct VolumeKnobProps {
    /// Where the value sits on its travel, 0 at the bottom of the sweep and
    /// 1 at the top. The *taper's* position, not the gain — see
    /// `daw_ui::controls::fader_position`.
    #[props(default)]
    pub value: f32,
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    #[props(default)]
    pub at: Interaction,
}

/// Where the value ring starts, clockwise from twelve o'clock, and how far
/// it runs.
///
/// A 320° track with a 40° gap at the bottom — measured off REAPER's,
/// whose ring closes much further round than the 300° first drawn here.
/// Public because the test that checks unity lands where the fader puts it
/// has to ask the same two numbers.
pub const VOLUME_KNOB_START: f32 = -160.0;
pub const VOLUME_KNOB_SWEEP: f32 = 320.0;

/// REAPER's track-panel volume knob: a dark body inside a value ring.
///
/// It is drawn rather than traced. REAPER's own is `tcp_vol_knob_stack`, a
/// 939-row sprite holding every position of the ring — there is no single
/// image to measure, and a strip of 40-odd frames is exactly the thing this
/// crate exists to stop shipping.
///
/// The geometry is read off a screenshot instead: a 21-wide knob whose ring
/// is open at the bottom, running from seven o'clock clockwise through
/// twelve to five — a 300° track with a 60° gap. Unity lands a little past
/// two o'clock, which is the same 0.708 of travel the fader puts it at, and
/// is the check that the sweep is right.
#[component]
pub fn VolumeKnob(props: VolumeKnobProps) -> Element {
    let _t = Theme::default();
    // 24, the height of the field it straddles. REAPER's knob fills that
    // band exactly — a knob two rows shorter than the box it sits on reads
    // as sunk into it rather than seated on its edge.
    let (vw, vh) = (24.0f32, 24.0f32);
    let (cx, cy) = (vw * 0.5, vh * 0.5);
    // Measured off REAPER pixel by pixel across the knob's centre row and
    // down its centre column: a 22-wide cell, the ring's outer edge at 11
    // from centre with a 4-wide stroke, and a body of 7.5. Drawn thinner —
    // a 2.9 stroke on a 5.8 body — the ring read as a hairline and the
    // knob as a smaller control than the one beside it.
    // The ring is inset by a dark rim — REAPER's knob starts with one
    // near-black column at 158 and only then the blue. Without it the ring
    // runs to the cell's edge and the knob reads a size larger than the
    // one beside it even when the two measure the same.
    // Absolute, not fractions of the cell. REAPER's knob is 22 across
    // inside a 24-tall field, so the cell is the field's height and the
    // drawing keeps its own measurements: an 11 rim, a 7.5 body and a 3.5
    // ring between them. Scaled to the cell instead, the ring came out
    // heavy and the body small — the knob read as a thick washer rather
    // than a cap with a track round it.
    let rim = 11.0f32;
    let r = 9.0f32;
    let stroke = 3.5f32;
    let body = 7.5f32;

    /// A point on the ring, by angle clockwise from twelve o'clock.
    fn on(cx: f32, cy: f32, r: f32, deg: f32) -> (f32, f32) {
        let a = deg.to_radians();
        (cx + r * a.sin(), cy - r * a.cos())
    }

    let (start, sweep) = (VOLUME_KNOB_START, VOLUME_KNOB_SWEEP);
    let arc = |from: f32, to: f32| {
        let (x0, y0) = on(cx, cy, r, from);
        let (x1, y1) = on(cx, cy, r, to);
        let large = if (to - from).abs() > 180.0 { 1 } else { 0 };
        format!("M {x0} {y0} A {r} {r} 0 {large} 1 {x1} {y1}")
    };

    let value = props.value.clamp(0.0, 1.0);
    let lit_to = start + sweep * value;
    let ink = ink(None, props.at, true, 0.35);

    rsx! {
        svg {
            width: "{props.width.unwrap_or(vw as u32)}",
            height: "{props.height.unwrap_or(vh as u32)}",
            style: "display:block; width:{props.width.unwrap_or(vw as u32)}px; \
                    height:{props.height.unwrap_or(vh as u32)}px;",
            view_box: "0 0 {vw} {vh}",
            xmlns: "http://www.w3.org/2000/svg",

            defs {
                // The same drop the pan knob casts, so the two controls
                // sit on the row the same way. REAPER's knob is not flat
                // against the field: it throws a soft shadow below it and
                // is ringed in near-black, which is most of what makes it
                // read as a knob rather than a printed circle.
                radialGradient { id: "voldrop",
                    stop { offset: "0.90", stop_color: "#000000", stop_opacity: "0.15" }
                    stop { offset: "1", stop_color: "#000000", stop_opacity: "0" }
                }
            }
            ellipse {
                cx: "{cx}", cy: "{cy + vh * 0.055}",
                rx: "{rim * 1.07}", ry: "{rim * 1.23}",
                fill: "url(#voldrop)",
            }
            // The black outline, and the rim the ring is inset into.
            circle { cx: "{cx}", cy: "{cy}", r: "{rim}", fill: "#0d0d0d" }
            defs {
                // The ring is not one flat blue. Sampled round REAPER's,
                // it runs the lit token where the value starts at the lower
                // left and the top token by the time it reaches the top —
                // lit from below, like the rest of this theme's hardware.
                linearGradient { id: "volring", x1: "0", y1: "1", x2: "0", y2: "0",
                    stop { offset: "0", stop_color: daw_theme::defaults::VOLUME_RING_LIT }
                    stop { offset: "1", stop_color: daw_theme::defaults::VOLUME_RING_LIT_TOP }
                }
                // And the body is not flat either: #303030 at the top to
                // #2D2D2D at the bottom. Three units, which sounds like
                // nothing and is the difference between a moulded cap and
                // a filled circle.
                linearGradient { id: "volbody", x1: "0", y1: "0", x2: "0", y2: "1",
                    stop { offset: "0", stop_color: "#303030" }
                    stop { offset: "1", stop_color: "#2d2d2d" }
                }
            }
            // The unlit track, all the way round.
            path {
                d: "{arc(start, start + sweep)}",
                fill: "none",
                // Measured off the unlit side of REAPER's ring — a slate
                // that stays legible against the body. Derived from the
                // accent it was too dark to see, so the knob looked like a
                // lit arc floating on nothing.
                stroke: daw_theme::defaults::VOLUME_RING_UNLIT,
                stroke_width: "{stroke}",
                stroke_linecap: "butt",
            }
            // The body, inside it.
            circle { cx: "{cx}", cy: "{cy}", r: "{body}", fill: "url(#volbody)" }
            circle {
                cx: "{cx}", cy: "{cy}", r: "{body}",
                fill: "none",
                stroke: "{ink.border.css()}",
                stroke_width: "0.8",
            }
            // The value, over the track. Drawn last so its cap sits on top
            // of the unlit stroke rather than under it.
            if value > 0.0 {
                path {
                    d: "{arc(start, lit_to)}",
                    fill: "none",
                    stroke: "url(#volring)",
                    stroke_width: "{stroke}",
                    stroke_linecap: "butt",
                }
            }
        }
    }
}

#[cfg(test)]
mod volume_knob_tests {
    use super::*;

    fn render(value: f32) -> String {
        let mut dom = dioxus::prelude::VirtualDom::new_with_props(
            VolumeKnob,
            VolumeKnobProps {
                value,
                width: Some(21),
                height: Some(21),
                at: Interaction::Normal,
            },
        );
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    /// The ring is a track plus a value arc, and at rest there is no value
    /// arc at all — a knob at silence must not show a lit sliver.
    #[test]
    fn the_ring_lights_only_as_far_as_the_value() {
        let silent = render(0.0);
        assert_eq!(
            silent.matches("<path").count(),
            1,
            "a lit arc at zero:\n{silent}"
        );

        let unity = render(0.708);
        assert_eq!(unity.matches("<path").count(), 2, "no value arc:\n{unity}");
    }

    /// Unity lands a little past two o'clock, which is what says the 300°
    /// sweep and the fader's taper agree about where 0 dB is.
    ///
    /// Read back out of the drawing rather than compared with a literal:
    /// the cell's size is taken from the rendered `viewBox`, so resizing
    /// the knob cannot turn this into a failing test about nothing.
    #[test]
    fn unity_lands_past_two_oclock() {
        let html = render(0.708);

        // The cell, from the drawing itself.
        let vb = html.split("viewBox=\"0 0 ").nth(1).expect("a viewBox");
        let vw: f32 = vb.split_whitespace().next().unwrap().parse().unwrap();
        let c = vw * 0.5;

        // The value arc is the last path; its `A` ends at the value's angle.
        let last = html.rsplit("<path").next().expect("a value arc");
        let end = last
            .split(" A ")
            .nth(1)
            .expect("an arc")
            .split('"')
            .next()
            .unwrap();
        let n: Vec<f32> = end
            .split_whitespace()
            .filter_map(|w| w.parse().ok())
            .collect();
        let (x, y) = (n[n.len() - 2], n[n.len() - 1]);

        // Clockwise from twelve, which is how the component measures.
        let deg = (x - c).atan2(c - y).to_degrees();
        let wanted = VOLUME_KNOB_START + VOLUME_KNOB_SWEEP * 0.708;
        assert!(
            (deg - wanted).abs() < 1.0,
            "unity is at {deg:.1}°, not {wanted:.1}° — the sweep and the taper disagree"
        );
        assert!((60.0..70.0).contains(&deg), "that is not past two o'clock");
    }
}

// ── track panel: the left column's glyphs ───────────────────────────────

#[derive(Props, Clone, PartialEq)]
pub struct GlyphProps {
    /// The ink. REAPER draws both of these in the row's own colour
    /// darkened — measured #762D40 against a #9D3C55 tint, the same 0.75
    /// the input combo is — so they read as pressed into the panel rather
    /// than printed on it.
    pub colour: String,
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
}

/// The pin at the top of a track panel's left column.
///
/// A **pushpin at an angle**, traced off `custom_track_pin_off.png` —
/// not the five-pointed star it was first drawn as from its name. The
/// glyph in the art's 19-wide cell occupies x4..14.5, y3..13.5 and is
/// four parts: a handle leaning in from the top, a flat brim running
/// across, the round head under it, and a needle running out to the
/// bottom-right corner. Coordinates below are the traced cell's, shifted
/// by (-4,-3) into an 11x11 box.
#[component]
pub fn TrackPin(props: GlyphProps) -> Element {
    let (vw, vh) = (11.0f32, 11.0f32);
    // Handle, brim, needle — the head is the circle below.
    let d = "M 4.1 0 L 5.2 0.2 L 4.6 2.8 L 2.9 2.4 Z \
             M 0.3 3.3 L 9.8 2.8 L 9.8 5.2 L 0.3 5.7 Z \
             M 6.6 7.6 L 7.6 7.0 L 10.6 10.2 L 9.5 10.7 Z";

    rsx! {
        svg {
            width: "{props.width.unwrap_or(vw as u32)}",
            height: "{props.height.unwrap_or(vh as u32)}",
            style: "display:block;",
            view_box: "0 0 {vw} {vh}",
            xmlns: "http://www.w3.org/2000/svg",
            path { d: "{d}", fill: "{props.colour}" }
            circle { cx: "5.4", cy: "5.6", r: "3.0", fill: "{props.colour}" }
        }
    }
}

/// The folder mark at the bottom of that column.
///
/// A tab and a body, traced off `track_folder_off.png`'s first mark: the
/// tab is 4x2 with a **square** right edge — the slanted edge this was
/// first drawn with is not in the art — over a 9x5 body, all one flat
/// ink. (The strip's other two marks are plus signs, per the
/// three-marks-in-one-image convention folder strips use.)
#[component]
pub fn TrackFolder(props: GlyphProps) -> Element {
    let (vw, vh) = (9.0f32, 7.0f32);
    rsx! {
        svg {
            width: "{props.width.unwrap_or(vw as u32)}",
            height: "{props.height.unwrap_or(vh as u32)}",
            style: "display:block;",
            view_box: "0 0 {vw} {vh}",
            xmlns: "http://www.w3.org/2000/svg",
            // The tab, then the body under it — one outline, no diagonal.
            path {
                d: "M 0 0 H 4 V 2 H 9 V 7 H 0 Z",
                fill: "{props.colour}",
            }
        }
    }
}

#[cfg(test)]
mod glyph_tests {
    use super::*;
    use crate::render::render_svg as render;

    fn glyph(colour: &str) -> GlyphProps {
        GlyphProps {
            colour: colour.into(),
            width: None,
            height: None,
        }
    }

    /// A pushpin, not a star: a round head, and a needle that reaches the
    /// glyph's bottom-right corner — the two things the star had neither
    /// of. The star was ten line segments; the pushpin's path is three
    /// small quads.
    #[test]
    fn the_pin_is_a_pushpin_and_not_a_star() {
        let svg = render(TrackPin, glyph("#752d3f"));
        assert_eq!(svg.matches("<circle").count(), 1, "no round head:\n{svg}");
        let d = svg.split("d=\"").nth(1).unwrap().split('"').next().unwrap();
        assert_eq!(d.matches('M').count(), 3, "not three parts:\n{d}");
        assert!(
            d.contains("10.6 10.2"),
            "the needle does not reach the corner:\n{d}"
        );
    }

    /// The folder's tab is square — the art has no slanted edge, so the
    /// outline is axis-aligned from end to end: a 4x2 tab over a 9x5 body.
    #[test]
    fn the_folder_tab_is_square() {
        let svg = render(TrackFolder, glyph("#752d3f"));
        let d = svg.split("d=\"").nth(1).unwrap().split('"').next().unwrap();
        assert!(!d.contains('L'), "the tab has a diagonal again:\n{d}");
        for step in ["H 4", "V 2", "H 9", "V 7"] {
            assert!(d.contains(step), "the {step} edge moved:\n{d}");
        }
    }
}

/// The fixed-lanes button — `custom_fixed_lanes_*`, this theme's own
/// addition rather than one of REAPER's.
///
/// **Two stacked dots**, not a stack of bars. Traced off the image: a
/// 20x24 cell with dots of radius 2.5 centred on x9.5 at y9 and y17,
/// #818181 with the lower one at 0.40 alpha when off, and both a solid
/// #CBA336 when the track is in fixed-lane mode.
///
/// Drawn as bars first, it was indistinguishable from the routing button
/// two rows above it — which is a worse failure than being ugly, because
/// the two do entirely different things and the panel had them looking
/// the same.
#[derive(Props, Clone, PartialEq)]
pub struct FixedLanesProps {
    /// Whether the track is in fixed-lane mode.
    #[props(default)]
    pub on: bool,
    #[props(default)]
    pub width: Option<u32>,
    #[props(default)]
    pub height: Option<u32>,
    #[props(default)]
    pub at: Interaction,
}

#[component]
pub fn FixedLanesButton(props: FixedLanesProps) -> Element {
    let (vw, vh) = (20.0f32, 24.0f32);
    let (cx, r) = (vw * 0.475, 2.5f32);
    let t = Theme::default();
    // Hover and press lift the marks, as everything else here does.
    let lift = match props.at {
        Interaction::Normal => 0.0,
        Interaction::Hover => 0.12,
        Interaction::Pressed => -0.10,
    };
    let (upper, lower, lower_alpha) = if props.on {
        let lit = t.signal.solo.shade(lift).css();
        (lit.clone(), lit, 1.0f32)
    } else {
        let ink = Color::hex(daw_theme::defaults::FIXED_LANES_DOT)
            .expect("token is valid hex")
            .shade(lift)
            .css();
        (ink.clone(), ink, 0.40)
    };

    rsx! {
        svg {
            width: "{props.width.unwrap_or(vw as u32)}",
            height: "{props.height.unwrap_or(vh as u32)}",
            style: "display:block;",
            view_box: "0 0 {vw} {vh}",
            xmlns: "http://www.w3.org/2000/svg",
            circle { cx: "{cx}", cy: "9", r: "{r}", fill: "{upper}" }
            circle { cx: "{cx}", cy: "17", r: "{r}", fill: "{lower}", fill_opacity: "{lower_alpha}" }
        }
    }
}

#[cfg(test)]
mod fixed_lanes_tests {
    use super::*;

    fn render(on: bool) -> String {
        let mut dom = dioxus::prelude::VirtualDom::new_with_props(
            FixedLanesButton,
            FixedLanesProps {
                on,
                width: Some(20),
                height: Some(24),
                at: Interaction::Normal,
            },
        );
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    /// Two dots, and nothing that could be mistaken for the routing
    /// button's lanes — which is what it was drawn as first, two rows
    /// below the routing button itself.
    #[test]
    fn it_is_two_dots_and_not_a_stack_of_bars() {
        let off = render(false);
        assert_eq!(off.matches("<circle").count(), 2, "not two dots:\n{off}");
        assert_eq!(off.matches("<rect").count(), 0, "it has bars:\n{off}");
    }

    /// Lit, both dots take the theme's mark colour at full strength; unlit,
    /// the lower one is a shadow of the upper.
    #[test]
    fn the_lower_dot_is_faint_until_the_mode_is_on() {
        assert!(
            render(false).contains(r#"fill-opacity="0.4""#),
            "the dots read as equals"
        );
        assert!(
            render(true).contains(r#"fill-opacity="1""#),
            "the lit dots are not solid"
        );
    }
}
