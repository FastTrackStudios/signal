//! The heights at which REAPER's mixer strip changes shape.
//!
//! Facts about the theme, like [`crate::slice`] and [`crate::dress`]: the
//! numbers are REAPER's, read out of `rtconfig.txt`, and two very different
//! consumers need the same ones. `daw-ui` collapses a live strip by them;
//! `fts-themer` writes them back into the theme's layout file so the panel
//! and the host cannot drift apart about when a control disappears.
//!
//! What a component *does* with them — which box re-anchors where, when a
//! fader becomes a knob — is behaviour and lives in `daw_ui::controls`.

/// Every height at which the strip changes shape, in REAPER's own pixels.
///
/// One constant, read by the panel — and spliced into the theme's layout
/// file by #148, so the two sides cannot drift.
///
/// Taken from `rtconfig.txt`: the five container thresholds are the
/// `hide_*` parameters, the residual ones are `mcp_io_hide_h`,
/// `mcp_env_hide_h` and `mcp_phase_hide_h`, and the padding stages are
/// `padding_reduction_h`. The env and phase thresholds are pairs because
/// the theme carries one value per labels mode; the first is the mode this
/// strip draws.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Thresholds {
    /// Below this, the input-FX row goes.
    pub input_fx: f32,
    /// Below this, the record-input row goes.
    pub record_input: f32,
    /// Below this, the pan labels go.
    pub pan_labels: f32,
    /// Below this, the pan *section* goes — the control re-anchors.
    pub pan_section: f32,
    /// Below this, the fader's dB readout goes.
    pub volume_label: f32,
    /// Stretch-section height below which the IO button goes.
    pub io: f32,
    /// Stretch-section height below which the envelope button goes.
    pub envelope: f32,
    /// Stretch-section height below which the phase button goes.
    pub phase: f32,
    /// Stretch-section heights at which padding steps down: below the
    /// first it is 3px, below the second 2px, otherwise 4px.
    pub padding_steps: (f32, f32),
    /// Fader-area height below which the fader becomes a knob.
    ///
    /// REAPER's `min_vol_h`, and it is a threshold on the *fader area* —
    /// `set mcp.volume.fadermode vol_h<min_vol_h` — not on the strip. Read
    /// as a strip height it was 280, which turned the fader into a knob on
    /// strips where REAPER still draws a fader. See [`fader_area`].
    pub fader_swap: f32,
}

/// REAPER's values, at scale 1.
pub const REAPER: Thresholds = Thresholds {
    input_fx: 400.0,
    record_input: 350.0,
    pan_labels: 320.0,
    pan_section: 260.0,
    volume_label: 250.0,
    io: 106.0,
    envelope: 125.0,
    phase: 144.0,
    padding_steps: (350.0, 250.0),
    fader_swap: 45.0,
};

/// The height REAPER gives the fader itself, out of the stretch section.
///
/// `vol_t` starts below the IO button and `vol_b` stops above whichever of
/// phase / envelope / the name label comes first, so the fader gets what the
/// button column leaves — which is why the knob swap has to be asked of this
/// number and not of the strip.
///
/// The button heights are the ones `rtconfig` states in their own offset
/// chains: IO `[-1 23 23 30]`, envelope `[1 -22 21 22]`, phase the same.
pub fn fader_area(stretch: f32, io: bool, envelope: bool, phase: bool) -> f32 {
    const IO_H: f32 = 30.0;
    const ENV_H: f32 = 22.0;
    const PHASE_H: f32 = 22.0;
    // `vol_margin_t` 3 and `vol_margin_b` 3-or-1.
    const MARGINS: f32 = 4.0;
    let taken = if io { IO_H } else { 0.0 }
        + if envelope { ENV_H } else { 0.0 }
        + if phase { PHASE_H } else { 0.0 };
    (stretch - taken - MARGINS).max(0.0)
}

/// The fixed bands the stretch section is the residual of, in REAPER's
/// pixels.
///
/// Public because the theme states the same numbers — inside WALTER
/// expressions rather than as bare values — and `fts-themer` checks the two
/// agree. See `fts_themer::thresholds`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Bands {
    pub fx: f32,
    pub input_full: f32,
    pub bottom: f32,
}

/// REAPER's, from `fx_sec`, `in_sec` and `bot_sec_h`.
pub const REAPER_BANDS: Bands = Bands {
    fx: FX_SECTION,
    input_full: INPUT_SECTION_FULL,
    bottom: BOTTOM_SECTION,
};

/// The fixed bands, in REAPER's pixels, that the stretch section is what is
/// left over from. From `fx_sec`, `pan_sec`, `in_sec` and `bot_sec`.
pub const FX_SECTION: f32 = 33.0;
pub const PAN_SECTION_FULL: f32 = 50.0;
pub const PAN_SECTION_UNLABELLED: f32 = 33.0;
pub const PAN_SECTION_COLLAPSED: f32 = 6.0;
pub const INPUT_SECTION_FULL: f32 = 54.0;
pub const INPUT_SECTION_NO_FX: f32 = 42.0;
pub const INPUT_SECTION_MINIMAL: f32 = 22.0;
pub const BOTTOM_SECTION: f32 = 47.0;
