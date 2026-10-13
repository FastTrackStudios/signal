//! The transport theme context — REAPER's `trans.*` WALTER vocabulary.
//!
//! The transport is a WALTER layout context like the strips: buttons
//! (`trans.rew/fwd/play/stop/pause/rec/repeat/automode/timebase`), the BPM
//! group (`trans.bpm.edit/tap`, `trans.curtimesig`), the playrate fader
//! (`trans.rate(.fader)`), two text readouts (`trans.status` — play state +
//! position, `trans.sel` — time selection), `trans.custom.*` chrome, and
//! `trans.size` (+ `.dockedheight`/`.minmax`). Palette keys `col_trans_bg` /
//! `col_trans_fg` carry the base colours.

use super::mcp::{ButtonSkin, ButtonStateSkin, McpCustom, SkinImage};
use super::theme::Color;
use super::walter::{ColorPair, Coord, FontSpec, Margin};

/// One evaluated transport layout (a REAPER `Layout` block's `trans.*` set).
#[derive(Clone, PartialEq, Debug)]
pub struct TransLayout {
    pub name: String,
    /// `trans.size` — natural size; coords spring from it via their anchors.
    pub size: (f32, f32),
    /// `trans.size.dockedheight`.
    pub docked_height: f32,

    // ── buttons ──
    pub rew: Coord,
    pub fwd: Coord,
    pub play: Coord,
    pub stop: Coord,
    pub pause: Coord,
    pub rec: Coord,
    pub repeat: Coord,
    pub automode: Coord,
    pub timebase: Coord,

    // ── BPM group ──
    pub bpm_edit: Coord,
    pub bpm_edit_font: FontSpec,
    pub bpm_edit_color: Option<ColorPair>,
    pub bpm_tap: Coord,
    pub curtimesig: Coord,
    pub curtimesig_color: Option<ColorPair>,

    // ── playrate ──
    pub rate: Coord,
    pub rate_fader: Coord,

    // ── readouts ──
    /// `trans.status` — play state + position.
    pub status: Coord,
    pub status_font: FontSpec,
    pub status_color: Option<ColorPair>,
    pub status_margin: Margin,
    /// `trans.sel` — the time-selection readout.
    pub sel: Coord,
    pub sel_font: FontSpec,
    pub sel_color: Option<ColorPair>,

    /// `trans.custom.*` chrome, in paint order.
    pub customs: Vec<McpCustom>,
}

impl TransLayout {
    /// FTS fallback: a 36px bar with the classic button row + readout.
    pub fn fts_default() -> Self {
        Self {
            name: "default".to_string(),
            size: (1000.0, 36.0),
            docked_height: 36.0,
            rew: Coord::px(6.0, 4.0, 28.0, 28.0),
            fwd: Coord::px(34.0, 4.0, 28.0, 28.0),
            play: Coord::px(92.0, 4.0, 32.0, 28.0),
            stop: Coord::px(152.0, 4.0, 28.0, 28.0),
            pause: Coord::px(180.0, 4.0, 28.0, 28.0),
            rec: Coord::px(64.0, 4.0, 30.0, 28.0),
            repeat: Coord::px(122.0, 4.0, 28.0, 28.0),
            automode: Coord::hidden(),
            timebase: Coord::hidden(),
            bpm_edit: Coord::new(330.0, 8.0, 60.0, 20.0, 0.0, 0.0, 0.0, 0.0),
            bpm_edit_font: FontSpec::new(11.0, 600),
            bpm_edit_color: None,
            bpm_tap: Coord::hidden(),
            curtimesig: Coord::hidden(),
            curtimesig_color: None,
            rate: Coord::hidden(),
            rate_fader: Coord::hidden(),
            status: Coord::new(216.0, 4.0, 110.0, 28.0, 0.0, 0.0, 0.0, 0.0),
            status_font: FontSpec::new(11.0, 600),
            status_color: None,
            status_margin: Margin::new(6.0, 0.0, 6.0, 0.0, 0.0),
            sel: Coord::hidden(),
            sel_font: FontSpec::new(10.0, 500),
            sel_color: None,
            customs: Vec::new(),
        }
    }
}

/// Transport image skin — the `transport_*` atlas vocabulary.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct TransSkin {
    /// `transport_play` / `transport_play_on`.
    pub play: Option<ButtonSkin>,
    /// `transport_pause` / `transport_pause_on`.
    pub pause: Option<ButtonSkin>,
    /// `transport_record` / `transport_record_on`.
    pub rec: Option<ButtonSkin>,
    /// `transport_repeat_off` / `transport_repeat_on`.
    pub repeat: Option<ButtonSkin>,
    /// `transport_stop`.
    pub stop: Option<ButtonStateSkin>,
    /// `transport_previous` / `transport_next` (rew/fwd).
    pub rew: Option<ButtonStateSkin>,
    pub fwd: Option<ButtonStateSkin>,
    /// `transport_home` / `transport_end`.
    pub home: Option<ButtonStateSkin>,
    pub end: Option<ButtonStateSkin>,
    /// `transport_bpm_bg` — the BPM readout backdrop.
    pub bpm_bg: Option<SkinImage>,
    /// `transSectionBg` — section backdrop (Anti-Theme chrome).
    pub section_bg: Option<SkinImage>,
    /// `transRateFaderBg` / `transport_playspeedthumb`.
    pub rate_bg: Option<SkinImage>,
    pub rate_thumb: Option<SkinImage>,
}

/// The transport theme context.
#[derive(Clone, PartialEq, Debug)]
pub struct TransTheme {
    pub layouts: Vec<TransLayout>,
    /// `col_trans_bg` — bar background.
    pub bg: Color,
    /// `col_trans_fg` — bar foreground/text.
    pub fg: Color,
    pub skin: Option<TransSkin>,
}

impl TransTheme {
    pub fn fts_default() -> Self {
        Self {
            layouts: vec![TransLayout::fts_default()],
            bg: Color::rgba(0x18, 0x18, 0x1b, 255),
            fg: Color::rgba(0xa1, 0xa1, 0xaa, 255),
            skin: None,
        }
    }

    /// Resolve a layout by name; unknown/`None` falls back to the first.
    pub fn layout(&self, name: Option<&str>) -> &TransLayout {
        name.and_then(|n| self.layouts.iter().find(|l| l.name == n))
            .or(self.layouts.first())
            .expect("TransTheme has at least one layout")
    }
}

impl Default for TransTheme {
    fn default() -> Self {
        Self::fts_default()
    }
}
