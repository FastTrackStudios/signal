//! What each piece of REAPER art is, and which part of it may stretch.
//!
//! REAPER blits a theme image into a box the layout chose, and that box is
//! rarely the size the art was drawn at. What happens to the difference is
//! a property **of the art**: a fader rail lengthens, a fader's end caps do
//! not; the FX pill's flat run grows, its rounded ends do not; a button
//! scales whole.
//!
//! That knowledge used to live in two places at once — implicitly in each
//! component's viewBox fractions, and explicitly at whichever call site had
//! last needed a control at a size other than its source's, as a bespoke
//! prop — `FaderCapProps::full`, `FxControlProps::widen`. Two controls, two
//! patches, one call site each. A third would have needed a third. Both are
//! gone now; what replaced them is [`NamedArt::stack`] and [`NamedArt::row`],
//! which hand the question to the layout engine.
//!
//! Here it is one value per REAPER image: the box the art was drawn at, and
//! a [`Slice`] naming the band of each axis that carries the slack.
//!
//! # Why a table and not an associated const
//!
//! One component serves several images at several sizes.
//! [`crate::vector_controls::LabelButton`] alone draws mute, solo and FX,
//! in the mixer at 21x20 and in the track panel at 21x24. The source box
//! cannot belong to the component, so it belongs to the *name*, and the
//! names live here in one table that both the exporter and — later — the
//! `daw-ui` wrapper read.
//!
//! # Where the numbers come from
//!
//! Every one is measured, never chosen. REAPER's own convention is that
//! magenta `#ff00ff` pixels along an image's border mark the region that
//! must **not** stretch: a run in from the upper-left corner and a run back
//! from the lower-right one, per axis ([the WALTER image
//! reference](https://www.reaper.fm/sdk/walter/images.php)). So the art
//! being replaced already states its own slice, and
//! [`crate::derive::guides`] reads it back out.
//!
//! `cargo run -p daw-theme-art --example slices` prints the whole table as
//! measured; [`crate::export::slice_mismatches`] compares every entry below
//! against the art again, and `declared_slices_match_the_source_art` fails
//! on any disagreement. Art drawn into a band it does not belong in is then
//! a test failure rather than a bug that surfaces months later when someone
//! resizes a mixer.
//!
//! # This is the identity at the source size
//!
//! Nothing here changes what is drawn. At the source box a slice
//! decomposes into exactly the source box, so the export path is unaffected
//! by construction — the exporter goes on rendering at source size and
//! stamping the source's magenta verbatim. The declaration earns its keep
//! at every *other* size.

/// Which band of one axis absorbs the difference between the source box
/// and the box the art is drawn into.
///
/// [`Band::Fixed`] and [`Band::All`] are the same thing to REAPER, which
/// scales an unguided image whole either way. They differ in what a
/// Dioxus wrapper does with the slack, which is why both exist and why the
/// audit does not try to tell them apart — see the module note.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Band {
    /// Never stretches. Buttons, knobs and glyphs: the whole drawing
    /// scales together, keeping its proportions.
    Fixed,
    /// Stretches everywhere. Flat backgrounds, which have no feature that
    /// could smear.
    All,
    /// `lo..hi`, in the source box's own units, stretches; the art outside
    /// keeps its source size.
    Middle(f32, f32),
}

impl Band {
    /// Does this band carry a magenta guide, and where?
    ///
    /// The one question the source art can answer. `None` for both
    /// [`Band::Fixed`] and [`Band::All`], which are indistinguishable in
    /// the art.
    pub fn guided(self) -> Option<(f32, f32)> {
        match self {
            Self::Middle(lo, hi) => Some((lo, hi)),
            Self::Fixed | Self::All => None,
        }
    }
}

/// Which band of each axis stretches, in the source box's own units.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Slice {
    pub x: Band,
    pub y: Band,
}

impl Slice {
    /// Neither axis stretches — the drawing scales whole.
    pub const FIXED: Self = Self {
        x: Band::Fixed,
        y: Band::Fixed,
    };
    /// Both axes stretch everywhere — a flat plate.
    pub const ALL: Self = Self {
        x: Band::All,
        y: Band::All,
    };

    pub const fn new(x: Band, y: Band) -> Self {
        Self { x, y }
    }
}

/// One REAPER image: the box its art is drawn at, and how it stretches.
///
/// `source` is a **cell**, not a file. `mcp_mute_on` ships as three 21x20
/// buttons in one 63x20 PNG, and 21x20 is what the component draws.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct NamedArt {
    /// The REAPER image filename, without extension or scale directory.
    pub name: &'static str,
    /// The box the art is drawn at, in REAPER's own pixels.
    pub source: (f32, f32),
    /// Which band of each axis absorbs slack, in `source`'s units.
    pub slice: Slice,
}

/// One band of a sliced drawing, as a thing that can be laid out.
///
/// A [`Slice`] says which part of a drawing stretches. A `Pane` is that
/// answer turned into something a layout engine can execute: the window of
/// the source viewBox this band shows, and whether it takes the slack.
///
/// # Why a decomposition rather than a coordinate mapper
///
/// The obvious way to draw a nine-slice is to compute, per shape, where it
/// lands in the target box — the art asks "how tall am I?" and recalculates.
/// That means every component reimplements the slice arithmetic, gets it
/// subtly different, and needs a measured height to draw at all.
///
/// Decomposing instead makes the *layout engine* do it. Each band is drawn
/// at its own source coordinates into its own `<svg>`; the fixed bands are
/// sized in pixels and the stretch band is `flex: 1`. Taffy stretches it —
/// identically under Blitz and in a browser — and nothing measures anything.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Pane {
    /// The window of the source box this pane shows: `(x, y, w, h)`.
    pub view: (f32, f32, f32, f32),
    /// Does this pane absorb the slack? Exactly one pane of a stack does.
    pub grow: bool,
}

impl NamedArt {
    /// The drawing decomposed down its y axis, top to bottom.
    ///
    /// One pane for art that does not stretch vertically; three when a
    /// middle band does — fixed cap, stretchy run, fixed cap. A vertical
    /// flex stack of these *is* the nine-slice.
    ///
    /// The horizontal twin is [`NamedArt::row`].
    pub fn stack(&self) -> Vec<Pane> {
        let (w, h) = self.source;
        let whole = |grow| Pane {
            view: (0.0, 0.0, w, h),
            grow,
        };
        match self.slice.y {
            // Nothing to decompose: the drawing scales whole, or it is a
            // plate that stretches everywhere. Either way one pane, and it
            // takes whatever height it is given.
            Band::Fixed | Band::All => vec![whole(matches!(self.slice.y, Band::All))],
            Band::Middle(lo, hi) => vec![
                Pane {
                    view: (0.0, 0.0, w, lo),
                    grow: false,
                },
                Pane {
                    view: (0.0, lo, w, hi - lo),
                    grow: true,
                },
                Pane {
                    view: (0.0, hi, w, h - hi),
                    grow: false,
                },
            ],
        }
    }

    /// The drawing decomposed along its x axis, left to right.
    ///
    /// [`NamedArt::stack`] turned on its side, and what the FX pill needs:
    /// `mcp.fx` is 43 wide against 28 of art, and only the flat run before
    /// the seam grows. Scaled whole instead, the rounded end elongates into
    /// a notch and the `FX` glyph stretches with it.
    pub fn row(&self) -> Vec<Pane> {
        let (w, h) = self.source;
        match self.slice.x {
            Band::Fixed | Band::All => {
                vec![Pane {
                    view: (0.0, 0.0, w, h),
                    grow: matches!(self.slice.x, Band::All),
                }]
            }
            // A band that runs to an edge leaves a zero-width pane there,
            // which is a `<div>` of nothing — dropped rather than rendered.
            Band::Middle(lo, hi) => [
                Pane {
                    view: (0.0, 0.0, lo, h),
                    grow: false,
                },
                Pane {
                    view: (lo, 0.0, hi - lo, h),
                    grow: true,
                },
                Pane {
                    view: (hi, 0.0, w - hi, h),
                    grow: false,
                },
            ]
            .into_iter()
            .filter(|p| p.view.2 > 0.0)
            .collect(),
        }
    }

    const fn new(name: &'static str, source: (f32, f32), slice: Slice) -> Self {
        Self {
            name,
            source,
            slice,
        }
    }
}

/// Look one image up by name.
///
/// `None` for art outside this table's scope — the transport bar and the
/// envelope panel's own controls, which still carry their box as a prop.
pub fn art(name: &str) -> Option<NamedArt> {
    MCP_ART.iter().copied().find(|a| a.name == name)
}

/// Look one image up, or panic naming it.
///
/// For the exporter, which reaches the call only for names it has already
/// matched: a missing entry there is a table that has fallen behind the
/// components, not a runtime condition to handle.
pub fn expect_art(name: &str) -> NamedArt {
    art(name).unwrap_or_else(|| panic!("no NamedArt declared for {name}"))
}

use Band::{Fixed, Middle};

/// A drawing that scales whole in both axes.
const fn fixed(name: &'static str, source: (f32, f32)) -> NamedArt {
    NamedArt::new(name, source, Slice::FIXED)
}

/// The MCP's art, plus the track-panel twins its components also draw.
///
/// Scoped to the controls the mixer strip is built from. The transport bar
/// and the envelope panel's buttons are deliberately absent: they are a
/// separate surface, they are not part of the MCP work, and adding them
/// would mean measuring art nothing yet reads the measurement of.
///
/// Where a control has a track-panel twin it is here too, because the twin
/// is the *same component* and the prop carrying the source box has to be
/// one thing.
pub const MCP_ART: &[NamedArt] = &[
    // ── mute and solo ────────────────────────────────────────────────────
    //
    // Three-state strips, and the one control whose vertical guide marks
    // the button body rather than a corner: `track_mute_on` draws rows
    // 0..20 of 24 and the guide runs to row 21, which is why the three
    // empty rows below it are what stretches.
    //
    // The track panel's cells are **21 wide starting at x1**, not 22
    // starting at x0. Measured, not assumed: a viewBox one unit wider than
    // the cell it lands in squeezes every coordinate inward, a fraction of
    // a pixel at the edges and a whole one at the far side — the source of
    // the last stubborn `bottom -1`.
    fixed("mcp_mute_off", (21.0, 20.0)),
    fixed("mcp_mute_on", (21.0, 20.0)),
    fixed("mcp_solo_off", (21.0, 20.0)),
    fixed("mcp_solo_on", (21.0, 20.0)),
    fixed("mcp_solodefeat_on", (21.0, 20.0)),
    NamedArt::new(
        "track_mute_off",
        (21.0, 24.0),
        Slice::new(Middle(1.0, 20.0), Middle(21.0, 23.0)),
    ),
    NamedArt::new(
        "track_mute_on",
        (21.0, 24.0),
        Slice::new(Middle(1.0, 20.0), Middle(21.0, 23.0)),
    ),
    NamedArt::new(
        "track_solo_off",
        (21.0, 24.0),
        Slice::new(Middle(1.0, 20.0), Middle(21.0, 23.0)),
    ),
    NamedArt::new(
        "track_solo_on",
        (21.0, 24.0),
        Slice::new(Middle(1.0, 20.0), Middle(21.0, 23.0)),
    ),
    NamedArt::new(
        "track_solodefeat_on",
        (21.0, 24.0),
        Slice::new(Middle(1.0, 20.0), Middle(21.0, 23.0)),
    ),
    NamedArt::new(
        "tcp_solodefeat_on",
        (21.0, 24.0),
        Slice::new(Middle(1.0, 20.0), Middle(21.0, 23.0)),
    ),
    // ── the FX pill's labelled half ──────────────────────────────────────
    //
    // The one slice that was already known and
    // already patched by hand: `mcp.fx` is 43 wide against 28 of art, and
    // only the flat run before the seam grows — `Middle(23, 28)` says so
    // in the art's own units.
    NamedArt::new(
        "mcp_fx_empty",
        (28.0, 22.0),
        Slice::new(Middle(23.0, 28.0), Middle(14.0, 18.0)),
    ),
    NamedArt::new(
        "mcp_fx_norm",
        (28.0, 22.0),
        Slice::new(Middle(23.0, 28.0), Middle(14.0, 18.0)),
    ),
    NamedArt::new(
        "mcp_fx_dis",
        (28.0, 22.0),
        Slice::new(Middle(23.0, 28.0), Middle(14.0, 18.0)),
    ),
    NamedArt::new(
        "track_fx_empty",
        (20.0, 22.0),
        Slice::new(Middle(18.0, 19.0), Middle(5.0, 6.0)),
    ),
    NamedArt::new(
        "track_fx_norm",
        (20.0, 22.0),
        Slice::new(Middle(18.0, 19.0), Middle(5.0, 6.0)),
    ),
    NamedArt::new(
        "track_fx_dis",
        (20.0, 22.0),
        Slice::new(Middle(18.0, 19.0), Middle(5.0, 6.0)),
    ),
    // ── the FX pill's bypass half ────────────────────────────────────────
    //
    // `_v` is the mixer's and `_h` the track
    // panel's, despite both carrying the `track_` prefix — they are named
    // for their layout, not their panel.
    NamedArt::new(
        "track_fxempty_v",
        (18.0, 22.0),
        Slice::new(Middle(2.0, 3.0), Middle(1.0, 21.0)),
    ),
    NamedArt::new(
        "track_fxon_v",
        (18.0, 22.0),
        Slice::new(Middle(2.0, 3.0), Middle(1.0, 21.0)),
    ),
    NamedArt::new(
        "track_fxoff_v",
        (18.0, 22.0),
        Slice::new(Middle(2.0, 3.0), Middle(1.0, 21.0)),
    ),
    NamedArt::new(
        "track_fxempty_h",
        (16.0, 22.0),
        Slice::new(Middle(2.0, 3.0), Middle(4.0, 5.0)),
    ),
    NamedArt::new(
        "track_fxon_h",
        (16.0, 22.0),
        Slice::new(Middle(2.0, 3.0), Middle(4.0, 5.0)),
    ),
    NamedArt::new(
        "track_fxoff_h",
        (16.0, 22.0),
        Slice::new(Middle(2.0, 3.0), Middle(4.0, 5.0)),
    ),
    // ── record arm ───────────────────────────────────────────────────────
    //
    // No guides in either family: REAPER scales the button whole.
    fixed("mcp_recarm_off", (36.0, 24.0)),
    fixed("mcp_recarm_on", (36.0, 24.0)),
    fixed("mcp_recarm_norec", (36.0, 24.0)),
    fixed("mcp_recarm_auto", (36.0, 24.0)),
    fixed("mcp_recarm_auto_on", (36.0, 24.0)),
    fixed("mcp_recarm_auto_norec", (36.0, 24.0)),
    fixed("track_recarm_off", (20.0, 20.0)),
    fixed("track_recarm_on", (20.0, 20.0)),
    fixed("track_recarm_norec", (20.0, 20.0)),
    fixed("track_recarm_auto", (20.0, 20.0)),
    fixed("track_recarm_auto_on", (20.0, 20.0)),
    fixed("track_recarm_auto_norec", (20.0, 20.0)),
    // ── routing ──────────────────────────────────────────────────────────
    //
    // The mixer's is the tall one, and 29 of its 32 rows are pinned
    // — only the strip below the output lane takes slack.
    NamedArt::new(
        "mcp_io",
        (23.0, 32.0),
        Slice::new(Fixed, Middle(29.0, 31.0)),
    ),
    NamedArt::new(
        "mcp_io_dis",
        (23.0, 32.0),
        Slice::new(Fixed, Middle(29.0, 31.0)),
    ),
    NamedArt::new(
        "mcp_io_s",
        (23.0, 32.0),
        Slice::new(Fixed, Middle(29.0, 31.0)),
    ),
    NamedArt::new(
        "mcp_io_s_dis",
        (23.0, 32.0),
        Slice::new(Fixed, Middle(29.0, 31.0)),
    ),
    NamedArt::new(
        "mcp_io_r",
        (23.0, 32.0),
        Slice::new(Fixed, Middle(29.0, 31.0)),
    ),
    NamedArt::new(
        "mcp_io_r_dis",
        (23.0, 32.0),
        Slice::new(Fixed, Middle(29.0, 31.0)),
    ),
    NamedArt::new(
        "mcp_io_s_r",
        (23.0, 32.0),
        Slice::new(Fixed, Middle(29.0, 31.0)),
    ),
    NamedArt::new(
        "mcp_io_s_r_dis",
        (23.0, 32.0),
        Slice::new(Fixed, Middle(29.0, 31.0)),
    ),
    NamedArt::new(
        "track_io",
        (28.0, 22.0),
        Slice::new(Middle(3.0, 4.0), Middle(4.0, 5.0)),
    ),
    NamedArt::new(
        "track_io_dis",
        (28.0, 22.0),
        Slice::new(Middle(3.0, 4.0), Middle(4.0, 5.0)),
    ),
    NamedArt::new(
        "track_io_s",
        (28.0, 22.0),
        Slice::new(Middle(3.0, 4.0), Middle(4.0, 5.0)),
    ),
    NamedArt::new(
        "track_io_s_dis",
        (28.0, 22.0),
        Slice::new(Middle(3.0, 4.0), Middle(4.0, 5.0)),
    ),
    NamedArt::new(
        "track_io_r",
        (28.0, 22.0),
        Slice::new(Middle(3.0, 4.0), Middle(4.0, 5.0)),
    ),
    NamedArt::new(
        "track_io_r_dis",
        (28.0, 22.0),
        Slice::new(Middle(3.0, 4.0), Middle(4.0, 5.0)),
    ),
    NamedArt::new(
        "track_io_s_r",
        (28.0, 22.0),
        Slice::new(Middle(3.0, 4.0), Middle(4.0, 5.0)),
    ),
    NamedArt::new(
        "track_io_s_r_dis",
        (28.0, 22.0),
        Slice::new(Middle(3.0, 4.0), Middle(4.0, 5.0)),
    ),
    // ── input monitoring ─────────────────────────────────────────────────
    //
    // The track panel's is **15 wide, not 16**: the measured cell is
    // (origin 1, width 15), and a 16-wide viewBox squeezed into it puts
    // every coordinate half a pixel right of where it was measured.
    //
    // Its guides are the ones the cells cannot see — the two magenta
    // columns sit outside every cell of a 47px file whose art runs 1..46,
    // so the x axis comes out unguided even though the file has marks on
    // it. That is the truth about those marks, not a gap in the reading.
    fixed("mcp_monitor_off", (21.0, 20.0)),
    fixed("mcp_monitor_on", (21.0, 20.0)),
    fixed("mcp_monitor_auto", (21.0, 20.0)),
    NamedArt::new(
        "track_monitor_off",
        (15.0, 24.0),
        Slice::new(Fixed, Middle(2.0, 22.0)),
    ),
    NamedArt::new(
        "track_monitor_on",
        (15.0, 24.0),
        Slice::new(Fixed, Middle(2.0, 22.0)),
    ),
    NamedArt::new(
        "track_monitor_auto",
        (15.0, 24.0),
        Slice::new(Fixed, Middle(2.0, 22.0)),
    ),
    // ── phase ────────────────────────────────────────────────────────────
    fixed("mcp_phase_norm", (16.0, 18.0)),
    fixed("mcp_phase_inv", (16.0, 18.0)),
    fixed("track_phase_norm", (16.0, 20.0)),
    fixed("track_phase_inv", (16.0, 20.0)),
    // ── the pan and width knobs ──────────────────────────────────────────
    //
    // Pan and width share art — the two source PNGs
    // are byte-identical, because REAPER labels the control and the knob
    // itself is generic.
    fixed("mcp_pan_knob_small", (24.0, 25.0)),
    fixed("mcp_width_knob_small", (24.0, 25.0)),
    fixed("tcp_pan_knob_small", (24.0, 25.0)),
    fixed("tcp_width_knob_small", (24.0, 25.0)),
    fixed("mcp_pan_knob_large", (28.0, 29.0)),
    fixed("mcp_width_knob_large", (28.0, 29.0)),
    // ── the volume fader ─────────────────────────────────────────────────
    //
    // The reason the slice model exists. The rail's end
    // caps are 16 rows each and only the run between them lengthens, which
    // is what REAPER renders and what `FaderCapProps::full` got wrong.
    //
    // The thumb is the one image whose guides are drawn somewhere REAPER
    // does not read: five magenta rows down the *top* of its right-hand
    // column, where the documented convention puts the top guide on the
    // left. Read by the rule REAPER states, its top end is not pinned.
    NamedArt::new(
        "mcp_volbg",
        (23.0, 55.0),
        Slice::new(Middle(1.0, 22.0), Middle(16.0, 39.0)),
    ),
    NamedArt::new(
        "mcp_volthumb",
        (27.0, 53.0),
        Slice::new(Middle(0.0, 26.0), Middle(0.0, 48.0)),
    ),
    // ── the panel sliders ────────────────────────────────────────────────
    fixed("tcp_volbg", (19.0, 24.0)),
    NamedArt::new(
        "tcp_volthumb",
        (27.0, 29.0),
        Slice::new(Middle(0.0, 26.0), Middle(0.0, 28.0)),
    ),
    NamedArt::new(
        "mcp_panbg",
        (69.0, 11.0),
        Slice::new(Middle(5.0, 64.0), Middle(4.0, 8.0)),
    ),
    NamedArt::new(
        "mcp_widthbg",
        (69.0, 11.0),
        Slice::new(Middle(5.0, 64.0), Middle(4.0, 8.0)),
    ),
    NamedArt::new(
        "tcp_panbg",
        (43.0, 11.0),
        Slice::new(Middle(2.0, 41.0), Middle(3.0, 8.0)),
    ),
    NamedArt::new(
        "tcp_widthbg",
        (43.0, 11.0),
        Slice::new(Middle(2.0, 41.0), Middle(4.0, 9.0)),
    ),
    NamedArt::new(
        "mcp_panthumb",
        (13.0, 23.0),
        Slice::new(Middle(0.0, 12.0), Middle(0.0, 22.0)),
    ),
    NamedArt::new(
        "mcp_widththumb",
        (13.0, 23.0),
        Slice::new(Middle(0.0, 12.0), Middle(0.0, 22.0)),
    ),
    NamedArt::new(
        "tcp_panthumb",
        (13.0, 23.0),
        Slice::new(Middle(0.0, 12.0), Middle(0.0, 22.0)),
    ),
    NamedArt::new(
        "tcp_widththumb",
        (13.0, 23.0),
        Slice::new(Middle(0.0, 12.0), Middle(0.0, 22.0)),
    ),
    NamedArt::new(
        "mcp_folder_on",
        (55.0, 15.0),
        Slice::new(Middle(14.0, 54.0), Middle(13.0, 14.0)),
    ),
    // ── the FX and send lists ────────────────────────────────────────────
    //
    // Three pills stacked vertically: one drawing, not
    // a three-cell strip.
    NamedArt::new(
        "mcp_fxlist_norm",
        (38.0, 53.0),
        Slice::new(Middle(8.0, 30.0), Middle(7.0, 46.0)),
    ),
    NamedArt::new(
        "mcp_fxlist_byp",
        (38.0, 53.0),
        Slice::new(Middle(8.0, 30.0), Middle(7.0, 46.0)),
    ),
    NamedArt::new(
        "mcp_fxlist_off",
        (38.0, 53.0),
        Slice::new(Middle(8.0, 30.0), Middle(7.0, 46.0)),
    ),
    NamedArt::new(
        "mcp_fxlist_empty",
        (38.0, 53.0),
        Slice::new(Middle(8.0, 30.0), Middle(7.0, 46.0)),
    ),
    NamedArt::new(
        "mcp_sendlist_norm",
        (38.0, 50.0),
        Slice::new(Middle(9.0, 28.0), Middle(5.0, 41.0)),
    ),
    NamedArt::new(
        "mcp_sendlist_mute",
        (38.0, 50.0),
        Slice::new(Middle(7.0, 28.0), Middle(5.0, 41.0)),
    ),
    NamedArt::new(
        "mcp_sendlist_empty",
        (38.0, 50.0),
        Slice::new(Middle(7.0, 28.0), Middle(6.0, 41.0)),
    ),
    NamedArt::new(
        "mcp_sendlist_midihw",
        (38.0, 50.0),
        Slice::new(Middle(7.0, 28.0), Middle(5.0, 41.0)),
    ),
    // ── the panel plates ─────────────────────────────────────────────────
    //
    // Flat bands inside a frame REAPER holds still while it
    // stretches the inside across a whole strip. The envelope panel's
    // backgrounds are here because `PanelPlate` draws them too — the line
    // this table draws is the component, not the surface; the envelope
    // panel's own buttons keep their `cell` prop.
    NamedArt::new(
        "mcp_mainbg",
        (6.0, 6.0),
        Slice::new(Middle(1.0, 5.0), Middle(1.0, 5.0)),
    ),
    NamedArt::new(
        "mcp_mainbgsel",
        (6.0, 6.0),
        Slice::new(Middle(2.0, 5.0), Middle(1.0, 5.0)),
    ),
    NamedArt::new(
        "mcp_bg",
        (4.0, 4.0),
        Slice::new(Middle(1.0, 3.0), Middle(1.0, 3.0)),
    ),
    NamedArt::new(
        "mcp_bgsel",
        (4.0, 3.0),
        Slice::new(Middle(2.0, 3.0), Middle(1.0, 2.0)),
    ),
    NamedArt::new(
        "mcp_extmixbg",
        (3.0, 3.0),
        Slice::new(Middle(1.0, 2.0), Middle(1.0, 2.0)),
    ),
    NamedArt::new(
        "mcp_extmixbgsel",
        (3.0, 3.0),
        Slice::new(Middle(1.0, 2.0), Middle(1.0, 2.0)),
    ),
    fixed("mcp_mainextmixbg", (9.0, 8.0)),
    fixed("mcp_mainextmixbgsel", (9.0, 8.0)),
    NamedArt::new(
        "mcp_main_namebg",
        (8.0, 9.0),
        Slice::new(Middle(2.0, 7.0), Middle(4.0, 7.0)),
    ),
    fixed("mcp_main_namebg_sel", (8.0, 9.0)),
    NamedArt::new(
        "mcp_iconbg",
        (6.0, 11.0),
        Slice::new(Middle(2.0, 5.0), Middle(4.0, 7.0)),
    ),
    NamedArt::new(
        "mcp_iconbgsel",
        (6.0, 11.0),
        Slice::new(Middle(2.0, 5.0), Middle(4.0, 7.0)),
    ),
    fixed("tcp_mainbg", (22.0, 9.0)),
    fixed("tcp_mainbgsel", (22.0, 9.0)),
    fixed("tcp_iconbg", (11.0, 13.0)),
    fixed("tcp_iconbgsel", (11.0, 13.0)),
    fixed("tcp_idxbg", (23.0, 6.0)),
    fixed("tcp_idxbg_sel", (25.0, 6.0)),
    NamedArt::new(
        "envcp_bg",
        (48.0, 12.0),
        Slice::new(Middle(1.0, 15.0), Middle(1.0, 10.0)),
    ),
    NamedArt::new(
        "envcp_bgsel",
        (48.0, 12.0),
        Slice::new(Middle(1.0, 15.0), Middle(1.0, 10.0)),
    ),
    NamedArt::new(
        "envcp_namebg",
        (22.0, 24.0),
        Slice::new(Middle(2.0, 8.0), Middle(2.0, 22.0)),
    ),
    // ── the plates REAPER draws itself ───────────────────────────────────
    //
    // Every pixel is transparent, so there is nothing to hold still and
    // nothing to smear. `All` rather than `Fixed` — the art cannot tell
    // the two apart, and this is which of them it means.
    NamedArt::new("mcp_namebg", (3.0, 3.0), Slice::ALL),
    NamedArt::new("mcp_idxbg", (1.0, 1.0), Slice::ALL),
    NamedArt::new("mcp_idxbg_sel", (1.0, 1.0), Slice::ALL),
    NamedArt::new("tcp_namebg", (3.0, 3.0), Slice::ALL),
    NamedArt::new("tcp_main_namebg_sel", (3.0, 3.0), Slice::ALL),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_entry_is_declared_once() {
        let mut seen: Vec<&str> = MCP_ART.iter().map(|a| a.name).collect();
        seen.sort_unstable();
        let n = seen.len();
        seen.dedup();
        assert_eq!(seen.len(), n, "an image is declared twice");
    }

    #[test]
    fn a_declared_stretch_band_lies_inside_its_source_box() {
        for a in MCP_ART {
            for (axis, band, extent) in [("x", a.slice.x, a.source.0), ("y", a.slice.y, a.source.1)]
            {
                if let Some((lo, hi)) = band.guided() {
                    assert!(
                        lo <= hi && hi <= extent,
                        "{}: {axis} band {lo}..{hi} escapes a {extent}px source box",
                        a.name,
                    );
                }
            }
        }
    }

    /// Whatever a stack decomposes into must add back up to the source box,
    /// with no gap and no overlap — a pane boundary off by a row is a seam
    /// in the middle of a fader.
    #[test]
    fn a_stack_tiles_its_source_box_exactly() {
        for a in MCP_ART {
            let panes = a.stack();
            let mut y = 0.0;
            for p in &panes {
                assert_eq!(
                    p.view.1, y,
                    "{}: a pane starts off the last one's end",
                    a.name
                );
                assert!(p.view.3 >= 0.0, "{}: a pane has negative height", a.name);
                y += p.view.3;
            }
            assert_eq!(y, a.source.1, "{}: the stack does not fill its box", a.name);
            assert!(
                panes.iter().filter(|p| p.grow).count() <= 1,
                "{}: two panes both claim the slack",
                a.name,
            );
        }
    }

    /// The fader is why this exists: fixed caps, a stretchy run between.
    #[test]
    fn the_fader_rail_decomposes_into_cap_run_cap() {
        let panes = expect_art("mcp_volbg").stack();
        assert_eq!(panes.len(), 3);
        assert_eq!(panes[0].view, (0.0, 0.0, 23.0, 16.0));
        assert_eq!(panes[1].view, (0.0, 16.0, 23.0, 23.0));
        assert_eq!(panes[2].view, (0.0, 39.0, 23.0, 16.0));
        assert!(panes[1].grow, "the run between the caps takes the slack");
        assert!(!panes[0].grow && !panes[2].grow, "the caps hold their size");
    }

    #[test]
    fn lookup_finds_a_name_and_reports_one_it_does_not_have() {
        assert_eq!(art("mcp_mute_on").map(|a| a.source), Some((21.0, 20.0)));
        assert!(art("transport_play").is_none(), "transport is out of scope");
    }
}
