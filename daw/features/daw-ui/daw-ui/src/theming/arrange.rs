//! The arrange-view theme context: ruler, grid, lanes, items, cursors.
//!
//! Unlike the strip contexts (`tcp.*`/`mcp.*`), REAPER's arrange view and
//! time ruler carry **no WALTER layout** — they are themed entirely through
//! `.ReaperTheme` palette keys (`col_tl_*` ruler, `col_gridlines*` grid,
//! `col_tr1/2_*` row pairs, `col_mi_*` media items, cursor + marker/region
//! keys). [`ArrangeTheme`] is the typed mirror of that vocabulary, with FTS
//! dark defaults so a bare theme still renders.

use super::theme::Color;

/// Arrange + ruler colours — the `.ReaperTheme` arrange vocabulary.
/// Importers fill these from the palette (drawmode alphas pre-applied);
/// every field has a sensible FTS dark default.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ArrangeTheme {
    // ── backdrop ──
    /// `col_arrangebg` — the timeline backdrop behind the lanes.
    pub bg: Color,
    /// `col_tracklistbg` — the empty area below the last track.
    pub empty_bg: Color,
    /// `col_tr1_bg` / `col_tr2_bg` — alternating lane backgrounds
    /// (REAPER alternates per track index).
    pub row_bg: [Color; 2],
    /// `selcol_tr1_bg` / `selcol_tr2_bg` — lane backgrounds of *selected*
    /// tracks.
    pub sel_row_bg: [Color; 2],
    /// `col_tr1_divline` / `col_tr2_divline` — the lane divider lines.
    pub row_divider: [Color; 2],
    /// `arrange_vgrid` — vertical grid shading in the empty area below the
    /// last track.
    pub vgrid: Color,

    // ── grid (drawmode alpha applied) ──
    /// `col_gridlines2` (+ `col_gridlines2dm`) — start-of-measure lines.
    pub grid_measure: Color,
    /// `col_gridlines3` (+ `col_gridlines3dm`) — start-of-beat lines.
    pub grid_beat: Color,
    /// `col_gridlines` (+ `col_gridlines1dm`) — in-between-beat lines.
    pub grid_sub: Color,

    // ── time ruler ──
    /// `col_tl_bg` — ruler background.
    pub ruler_bg: Color,
    /// `col_tl_fg` — ruler text + primary tick marks.
    pub ruler_fg: Color,
    /// `col_tl_fg2` — secondary tick marks / minor labels.
    pub ruler_fg2: Color,
    /// `col_tl_bgsel` — the time-selection band in the ruler.
    pub ruler_sel_bg: Color,
    /// `col_tl_bgsel` + `timesel_drawmode` — the time-selection shading over
    /// the arrange body (alpha pre-applied).
    pub timesel: Color,
    /// `col_tl_bgsel2` — the ruler background inside loop points.
    pub ruler_loop_bg: Color,

    // ── cursors ──
    /// `col_cursor` — the edit cursor line.
    pub edit_cursor: Color,
    /// `playcursor_color` (+ `playcursor_drawmode`) — the play cursor line.
    pub play_cursor: Color,

    // ── media items ──
    /// `col_mi_bg` / `col_mi_bg2` — item body fallbacks (even / odd tracks).
    pub item_bg: [Color; 2],
    /// `itembg_drawmode` alpha — how strongly the item/track colour tints
    /// the item body over `item_bg` (1.0 = solid colour).
    pub item_blend: f32,
    /// `col_mi_label` — item label text.
    pub item_label: Color,
    /// `col_mi_label_sel` — selected-item label text.
    pub item_label_sel: Color,
    /// `col_peaksedge` — peak outline; doubles as the item border.
    pub item_edge: Color,
    /// `col_tr1_peaks` / `col_tr2_peaks` — waveform peak fills.
    pub peaks: [Color; 2],
    /// `col_tr1_itembgsel` / `col_tr2_itembgsel` — selected item bodies.
    pub item_bg_sel: [Color; 2],
    /// `selitem_tag` — the coloured bar drawn on selected items
    /// (`None`/0 = the theme disables the tag, REAPER's flag convention).
    pub selitem_tag: Option<Color>,
    /// `col_mi_fades` — fade handle/curve lines.
    pub fade_line: Color,
    /// `fadezone_color` (+ drawmode) — the fade-triangle fill.
    pub fadezone: Color,
    /// `mute_overlay_col` (+ mode) — overlay across muted items.
    pub mute_overlay: Color,

    // ── markers / regions (ruler lanes) ──
    /// `marker` — marker flag fill.
    pub marker: Color,
    /// `marker_edge` — marker edge line.
    pub marker_edge: Color,
    /// `marker_lane_bg` / `marker_lane_text`.
    pub marker_lane_bg: Color,
    pub marker_lane_text: Color,
    /// `region` — region band fill.
    pub region: Color,
    /// `region_edge` — region edge line.
    pub region_edge: Color,
    /// `region_lane_bg` / `region_lane_text`.
    pub region_lane_bg: Color,
    pub region_lane_text: Color,
    /// `col_tsigmark` — tempo/time-signature change marker.
    pub tsig: Color,
    /// `ts_lane_bg` / `ts_lane_text` — the tempo lane.
    pub ts_lane_bg: Color,
    pub ts_lane_text: Color,

    // ── envelopes ──
    /// `col_env1` — default envelope curve colour (per-envelope colours
    /// override).
    pub env_default: Color,
    /// `env_trim_vol` — volume-envelope curve colour.
    pub env_vol: Color,
    /// `col_envlane1/2_divline` — envelope-lane divider lines.
    pub envlane_divider: [Color; 2],

    // ── selections ──
    /// `areasel_fill` (+ `areasel_drawmode`) — razor/area selection fill.
    pub sel_fill: Color,
    /// `marquee_fill` (+ `marquee_drawmode`) — marquee selection fill.
    pub marquee_fill: Color,
}

impl ArrangeTheme {
    /// FTS dark defaults (match `Theme::dark`'s token family).
    pub fn fts_default() -> Self {
        let c = |hex: u32| {
            Color::rgba(
                ((hex >> 16) & 0xff) as u8,
                ((hex >> 8) & 0xff) as u8,
                (hex & 0xff) as u8,
                255,
            )
        };
        Self {
            bg: c(0x0a0a0c),
            empty_bg: c(0x09090b),
            row_bg: [c(0x121215), c(0x101013)],
            sel_row_bg: [c(0x1c1c22), c(0x1a1a20)],
            row_divider: [c(0x1d1d21), c(0x1d1d21)],
            vgrid: Color::rgba(255, 255, 255, 8),
            grid_measure: Color::rgba(255, 255, 255, 36),
            grid_beat: Color::rgba(255, 255, 255, 20),
            grid_sub: Color::rgba(255, 255, 255, 10),
            ruler_bg: c(0x18181b),
            ruler_fg: c(0xa1a1aa),
            ruler_fg2: c(0x71717a),
            ruler_sel_bg: Color::rgba(56, 189, 248, 64),
            timesel: Color::rgba(255, 255, 255, 24),
            ruler_loop_bg: c(0x27272a),
            edit_cursor: c(0x38bdf8),
            play_cursor: Color::rgba(255, 255, 255, 160),
            item_bg: [c(0x3f3f46), c(0x3f3f46)],
            item_blend: 1.0,
            item_label: c(0x0c0c0f),
            item_label_sel: c(0xffffff),
            item_edge: c(0x0a0a0c),
            peaks: [c(0x18181b), c(0x18181b)],
            item_bg_sel: [c(0x52525b), c(0x52525b)],
            selitem_tag: None,
            fade_line: c(0x791111),
            fadezone: Color::rgba(0, 0, 0, 60),
            mute_overlay: Color::rgba(0, 0, 0, 110),
            marker: c(0xef4444),
            marker_edge: c(0x71717a),
            marker_lane_bg: c(0x18181b),
            marker_lane_text: c(0xa1a1aa),
            region: c(0x22c55e),
            region_edge: c(0x71717a),
            region_lane_bg: c(0x18181b),
            region_lane_text: c(0xa1a1aa),
            tsig: c(0xeab308),
            ts_lane_bg: c(0x18181b),
            ts_lane_text: c(0xa1a1aa),
            env_default: c(0x22c55e),
            env_vol: c(0x38bdf8),
            envlane_divider: [c(0x1d1d21), c(0x1d1d21)],
            sel_fill: Color::rgba(56, 189, 248, 40),
            marquee_fill: Color::rgba(56, 189, 248, 40),
        }
    }
}

impl Default for ArrangeTheme {
    fn default() -> Self {
        Self::fts_default()
    }
}

/// Image chrome for the arrange view — REAPER 7's fixed-lane art. Kept
/// separate from [`ArrangeTheme`] (which stays `Copy`): these carry data
/// URIs. All optional; renderers fall back to colour-drawn equivalents.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct ArrangeSkin {
    /// `lane_solo_on/off` — the per-lane play button (normal frame of the
    /// 3-state strip). "On" = the lane is playing (REAPER's filled dot).
    pub lane_solo_on: Option<super::mcp::SkinImage>,
    pub lane_solo_off: Option<super::mcp::SkinImage>,
    /// `lane_solo_on/off_indicator` — the tiny variant drawn when the
    /// track shows only the playing lane.
    pub lane_solo_on_indicator: Option<super::mcp::SkinImage>,
    pub lane_solo_off_indicator: Option<super::mcp::SkinImage>,
    /// `fixed_lanes_one/small/big/hidden` — the lane display-mode button
    /// (cycles: show one lane → small lanes → big lanes).
    pub fixed_lanes_one: Option<super::mcp::SkinImage>,
    pub fixed_lanes_small: Option<super::mcp::SkinImage>,
    pub fixed_lanes_big: Option<super::mcp::SkinImage>,
    pub fixed_lanes_hidden: Option<super::mcp::SkinImage>,
}
