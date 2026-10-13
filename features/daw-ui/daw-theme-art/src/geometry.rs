//! Where REAPER's panels put their controls, in the theme's own pixels.
//!
//! Facts about the theme, like [`crate::collapse`] and [`crate::dress`]:
//! one home per panel, so a remeasurement changes one number in one place
//! and every consumer moves together. `daw-ui`'s panels lay out on these;
//! `fts-themer` checks the parseable ones against `rtconfig.txt` so an
//! edit to the theme cannot silently strand the panels on stale numbers
//! (see `fts_themer::thresholds::offsets_agree`).
//!
//! Three kinds of number live here, and each doc comment says which:
//!
//! - **Stated** — written in `rtconfig.txt` as a literal this crate can
//!   point at, usually a `[dx dy w h]` offset. These are the ones the
//!   themer guard covers.
//! - **Measured** — read off a REAPER screenshot during convergence,
//!   because the theme states them only through runtime terms (`padding`,
//!   `lscale`) WALTER resolves at draw time. The screenshots live in
//!   `features/daw-ui/reference/`.
//! - **Chosen** — a Dioxus-side decision with no REAPER counterpart,
//!   kept because a different value read worse.
//!
//! The *collapse* numbers — section heights and hide thresholds — stay in
//! [`crate::collapse`]; this module is position, not shape.

/// The mixer strip (`mcp_*`), at scale 1 in wide mode.
///
/// **Wide mode only, by decision (#245).** REAPER also has `narrowMode` —
/// a 54-wide strip where `rtconfig` re-anchors pan, width and the record
/// input — and the two reference screenshots disagree about the pan
/// section because one of them is in it. Width variants are deferred
/// until the wide strip has converged in REAPER itself (#212); until
/// then a disagreement between the references is a mode, not an error.
pub mod mcp {
    /// Stated: `set mcp_w + * scale ?narrowMode 54 86 …` — the wide
    /// strip is 86; narrow mode's 54 is not modelled (#245).
    pub const STRIP_W: f32 = 86.0;

    /// Measured: `mcp.fx` is nominally `[7 7 43 20]` of the section, but
    /// against REAPER with the coloured bands aligned its pill's face runs
    /// 11..26 of the section and ours ran 9..24 — the same 16 rows, two
    /// high. The section carries a top inset the box coordinates do not
    /// mention, so the pill goes at 9, not 7.
    pub const FX_PILL_TOP: f32 = 9.0;

    /// Measured: the meter's block, scale included. It starts at x=4 and
    /// REAPER's fader cap starts at 30, so 26 is the room there is.
    pub const METER_W: u32 = 26;

    /// Stated: `mcp.recinput … [6 0 75 16]` and `mcp.recmode … [6 4 42 16]`
    /// are both 16 rows.
    pub const INPUT_FIELD_H: f32 = 16.0;

    /// Stated: `mcp.pan`'s cell is 24 wide.
    pub const PAN_KNOB_W: f32 = 24.0;

    /// Chosen: `mcp.recmode`'s field. Wide enough for the word and the
    /// caret with a gap between them — at the stated 42 with a 12-column
    /// right pad the two were almost touching.
    pub const IN_FIELD_W: f32 = 38.0;

    /// Measured: the axis the right-hand column centres on — `mcp.recmon`
    /// and everything anchored to it. The record arm's ring sits at 0.486
    /// of its own 36-wide cell rather than in the middle, which is why the
    /// arm is placed separately (see [`ARM_LEFT`]).
    pub const COLUMN: f32 = 55.0;

    /// Stated: `mcp.recmon`, `mcp.mute` and `mcp.solo` are all 21 wide —
    /// the third element of `[7 20 21 20]`, `[0 19 21 20]`, `[0 21 21 20]`.
    /// Stated because the column has to *be* that wide: left to
    /// shrink-wrap, `align-items: center` centres every button on the
    /// widest child — the IO button, which `rtconfig` writes as
    /// `mcp.solo + [-1 23 23 30]`, deliberately one column left and two
    /// wider — and the whole stack drifted a pixel right of the record arm
    /// above it.
    pub const BUTTON_W: f32 = 21.0;

    /// The one vertical axis the arm and the column share.
    pub const COLUMN_AXIS: f32 = COLUMN + BUTTON_W / 2.0;

    /// Stated: `set mcp.recarm + * scale [0 0 36 24] …` — a 36x24 cell.
    pub const ARM_CELL_W: f32 = 36.0;
    /// Stated: the same cell's height.
    pub const ARM_CELL_H: f32 = 24.0;

    /// Measured: the ring is centred at 0.486 of the cell, not at 18 — so
    /// the cell's left edge is not the axis minus half its width.
    pub const ARM_LEFT: f32 = COLUMN_AXIS - ARM_CELL_W * 0.486;

    /// How far the record arm's housing hangs below the coloured band.
    ///
    /// Derived, not chosen: the housing flares out at 45° and then goes
    /// vertical at [`HOUSING_SHOULDER`][crate::vector_controls::HOUSING_SHOULDER].
    /// Everything below that is a plain rectangle, and REAPER sinks exactly
    /// that rectangle into the dark — so the flare emerges from the
    /// background instead of the button sitting on top of the colour with a
    /// seam under it.
    ///
    /// Measuring it off a screenshot undercounts: below the band the
    /// housing is dark on dark, which is the whole point of it.
    pub const ARM_OVERHANG: f32 = ARM_CELL_H * (1.0 - crate::vector_controls::HOUSING_SHOULDER);

    /// The chain `rtconfig` writes down the right-hand column, each step a
    /// `[dx dy w h]` offset from the control above it (plus `padding`,
    /// which [`crate::collapse`] resolves):
    ///
    /// ```text
    /// set mcp.recmon  + + [0 padding] [mcp.recarm mcp.recarm] * scale [7 20 21 20]
    /// set mcp.mute    + + [0 padding] [mcp.recmon mcp.recmon] * scale [0 19 21 20]
    /// set mcp.solo    + + [0 padding] [mcp.mute mcp.mute]     * scale [0 21 21 20]
    /// set mcp.io      … + + [0 padding] [mcp.solo mcp.solo]   * scale … [-1 23 23 30]
    /// ```
    ///
    /// The steps are not equal — 19 against a 20-tall button is a one-row
    /// *overlap* before padding — which is why the column is an offset
    /// chain and not a flex gap.
    pub const RECMON_FROM_ARM: f32 = 20.0;
    /// Stated: `mcp.mute`'s `[0 19 21 20]`.
    pub const MUTE_FROM_RECMON: f32 = 19.0;
    /// Stated: `mcp.solo`'s `[0 21 21 20]`.
    pub const SOLO_FROM_MUTE: f32 = 21.0;
    /// Stated: `mcp.io`'s `[-1 23 23 30]`.
    pub const IO_FROM_SOLO: f32 = 23.0;

    // The rest of each quad — heights, widths, the dx steps. Facts too,
    // and named here so the themer guard asserts values that have a home
    // to fix when the theme moves (a literal that exists only inside the
    // guard is a check nobody can act on).
    /// Stated: the fourth element of recmon/mute/solo's quads — the
    /// buttons are 20 tall.
    pub const BUTTON_H: f32 = 20.0;
    /// Stated: `mcp.recmon`'s dx off the arm, `[7 20 21 20]`.
    pub const RECMON_DX: f32 = 7.0;
    /// Stated: `mcp.io`'s own box from `[-1 23 23 30]` — one column left,
    /// 23 wide, 30 tall.
    pub const IO_DX: f32 = -1.0;
    pub const IO_W: f32 = 23.0;
    pub const IO_H: f32 = 30.0;
    /// Stated: `mcp.env`'s `[1 -30 21 30]`.
    pub const ENV_DX: f32 = 1.0;
    pub const ENV_W: f32 = 21.0;
    /// Stated: `mcp.phase`'s `[3 -18 16 18]`.
    pub const PHASE_DX: f32 = 3.0;
    /// Stated: `mcp.recinput`'s `[6 0 75 16]`.
    pub const RECINPUT_X: f32 = 6.0;
    pub const RECINPUT_W: f32 = 75.0;

    /// Env hangs 30 above the stretch section's floor, which is why the
    /// column spreads as a strip grows instead of staying a cluster at
    /// the top. Stated:
    ///
    /// ```text
    /// set mcp.env … + [0 stretch_sec{3}] + [mcp.io stretch_sec] * scale … [1 -30 21 30]
    /// ```
    pub const ENV_FROM_FLOOR: f32 = 30.0;
    /// Phase sits 18 above env, 16 wide. Stated:
    ///
    /// ```text
    /// set mcp.phase … + [mcp.env mcp.env] - * scale [3 -18 16 18] padding
    /// ```
    pub const PHASE_FROM_ENV: f32 = 18.0;
    /// Stated: the phase glyph's width, from the same `[3 -18 16 18]`.
    pub const PHASE_W: f32 = 16.0;

    /// Measured: `mcp.label` — 26 of the bottom section's 47, above the
    /// 20-row index plate and the row that divides them. The theme writes
    /// it through `label_sec * lscale`, a runtime term.
    pub const NAME_PLATE: u32 = 26;
}

/// The track control panel (`tcp_*`), measured off REAPER itself rather
/// than off the panel sheet.
///
/// The sheet is a useful reference for *drawing* the controls, but it puts
/// the meter inside the tint, and REAPER does not: with `meterRight` set —
/// which this theme sets — `rtconfig` moves the whole meter section to the
/// right edge of the panel, past mute and solo.
pub mod tcp {
    /// Measured: REAPER's default row height for this theme.
    pub const ROW_H: f32 = 70.0;
    /// Measured: a row's tint ends at 296; everything after it is REAPER's
    /// meter section.
    pub const TINT_W: f32 = 296.0;
    /// Measured: the meter section — mute and solo, then the meter at the
    /// very end. The gutter runs to 343.
    pub const GUTTER_W: f32 = 47.0;
    pub const ROW_W: f32 = TINT_W + GUTTER_W;
    /// Measured: mute and solo occupy 318..339 against a section starting
    /// at 297 — 21 into the section.
    pub const GUTTER_BUTTON_X: f32 = 21.0;
    /// Measured: solo's top, at 25 of the row — mute is 21 above it.
    pub const SOLO_TOP: f32 = 25.0;

    /// Stated, as a formula the theme writes out:
    ///
    /// ```text
    /// phaseHide_h        = 12 + element_h + tcp.solo{1} + tcp.solo{3}
    /// phaseHide_h       += 17 when the lanes button is shown
    /// fixed_lanes_hide_h = phaseHide_h - 17
    /// tcp.phase              = [tcp.solo meter_sec{3}] + [3 -24 16 20]
    /// tcp.custom.fixed_lanes = [tcp.solo meter_sec{3}] + [1 -47 20 24]
    /// ```
    ///
    /// Both hang off the *bottom* of the meter section, which is why they
    /// end up in the row's bottom-right corner with the lanes button above
    /// phase. A 70-row row is below both thresholds and shows neither —
    /// REAPER's own rows at that height do not have them either.
    pub const PHASE_HIDE_H: f32 = 12.0 + 20.0 + SOLO_TOP + 20.0 + 17.0;
    pub const LANES_HIDE_H: f32 = PHASE_HIDE_H - 17.0;
    /// Stated: `tcp.phase`'s `[3 -24 16 20]`.
    pub const PHASE_FROM_FLOOR: f32 = 24.0;
    /// Stated: `tcp.custom.fixed_lanes`'s `[1 -47 20 24]`.
    pub const LANES_FROM_FLOOR: f32 = 47.0;

    /// Measured: the meter, a vertical strip at the *start* of the meter
    /// section at 297..316 — grey at rest, which is why it is easy to
    /// mistake for the section's own background.
    pub const METER_X: f32 = 1.0;
    pub const METER_W: u32 = 19;

    /// Measured across one of REAPER's rows at a scanline above the text:
    /// the name field runs 33..161, the volume knob follows it at 159..180,
    /// pan at 183..207, the routing widget 214..239 and the FX pill
    /// 248..283, with the tint ending at 296.
    pub const NAME_FIELD_X: f32 = 33.0;
    pub const NAME_FIELD_W: f32 = 136.0;
    /// Measured: 24 tall, not the 17 the other fields are.
    pub const NAME_FIELD_H: f32 = 24.0;

    /// Measured: the field stops *midway through* the knob, and its right
    /// edge is square. Two scanlines that looked like a field wrapping the
    /// knob say this instead: above the knob's centre the dark ends at 161,
    /// and across the centre it runs to 179 — which is the knob's own body,
    /// not the field. So the knob is a plain circle sitting half on the
    /// field and half on the tint, centred on the field's right edge — 24
    /// wide from 157, the same 24 the field is tall.
    pub const VOLUME_KNOB_X: f32 = 157.0;
    /// Measured: centred at 195, between the field's end and the routing
    /// widget — its chord runs 191..200 at a row above the knobs' centres.
    pub const PAN_KNOB_X: f32 = 184.0;
    /// Measured: the one-pixel rule between the left column and the row.
    pub const COLUMN_RULE_X: f32 = 20.0;
    pub const ROUTING_X: f32 = 214.0;
    pub const FX_IN_X: f32 = 248.0;

    /// Measured: both rows of fields sit at these tops.
    pub const ROW_ONE: f32 = 6.0;
    pub const ROW_TWO: f32 = 34.0;
    /// Measured: 20 — the sheet's 17 left a three-row gap under every
    /// field in the second row.
    pub const FIELD_H: f32 = 20.0;
}
