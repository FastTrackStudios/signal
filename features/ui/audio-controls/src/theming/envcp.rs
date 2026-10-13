//! The envelope-control-panel theme context — REAPER's `envcp.*` vocabulary.
//!
//! An ECP row sits under its parent TCP row (one per visible envelope):
//! `envcp.label`, the value fader (`envcp.fader` + `.fadermode`), the value
//! readout (`envcp.value`), and the button cluster (`envcp.arm`,
//! `envcp.bypass`, `envcp.hide`, `envcp.learn`, `envcp.mod`), plus
//! `envcp.custom.*` chrome and `envcp.size` (`[default w, default h,
//! min w]`). Images ship as `envcp_*` atlases.

use super::mcp::{ButtonSkin, ButtonStateSkin, KnobSkin, McpCustom, SkinImage};
use super::walter::{ColorPair, Coord, FaderMode, FontSpec, Margin};

/// One evaluated ECP layout.
#[derive(Clone, PartialEq, Debug)]
pub struct EnvcpLayout {
    pub name: String,
    /// `envcp.size` — natural panel size.
    pub size: (f32, f32),
    pub min_w: f32,

    /// `envcp.label` (+ font/margin/color).
    pub label: Coord,
    pub label_font: FontSpec,
    pub label_margin: Margin,
    pub label_color: Option<ColorPair>,

    /// `envcp.fader` + `.fadermode` (1 forces a knob).
    pub fader: Coord,
    pub fader_mode: FaderMode,

    /// `envcp.value` (+ font/margin/color) — the value readout.
    pub value: Coord,
    pub value_font: FontSpec,
    pub value_margin: Margin,
    pub value_color: Option<ColorPair>,

    /// Buttons.
    pub arm: Coord,
    pub bypass: Coord,
    pub hide: Coord,
    pub learn: Coord,
    pub modulate: Coord,

    /// `envcp.custom.*` chrome, in paint order.
    pub customs: Vec<McpCustom>,
}

impl EnvcpLayout {
    /// FTS fallback: a slim horizontal row (label | fader | value | buttons).
    pub fn fts_default() -> Self {
        Self {
            name: "default".to_string(),
            size: (300.0, 40.0),
            min_w: 120.0,
            label: Coord::new(6.0, 4.0, 120.0, 14.0, 0.0, 0.0, 0.0, 0.0),
            label_font: FontSpec::new(10.0, 600),
            label_margin: Margin::new(2.0, 0.0, 4.0, 0.0, 0.0),
            label_color: None,
            fader: Coord::new(6.0, 22.0, 160.0, 12.0, 0.0, 0.0, 1.0, 0.0),
            fader_mode: FaderMode::Horizontal,
            value: Coord::new(180.0, 22.0, 60.0, 12.0, 1.0, 0.0, 1.0, 0.0),
            value_font: FontSpec::new(9.0, 500),
            value_margin: Margin::new(0.0, 0.0, 0.0, 0.0, 0.0),
            value_color: None,
            arm: Coord::px(132.0, 4.0, 16.0, 14.0),
            bypass: Coord::px(152.0, 4.0, 16.0, 14.0),
            hide: Coord::px(172.0, 4.0, 16.0, 14.0),
            learn: Coord::hidden(),
            modulate: Coord::hidden(),
            customs: Vec::new(),
        }
    }
}

/// `envcp_*` image atlases.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct EnvcpSkin {
    /// `envcp_arm_off/on`.
    pub arm: Option<ButtonSkin>,
    /// `envcp_bypass_off/on`.
    pub bypass: Option<ButtonSkin>,
    /// `envcp_hide`.
    pub hide: Option<ButtonStateSkin>,
    /// `envcp_learn(_on)`.
    pub learn: Option<ButtonSkin>,
    /// `envcp_parammod(_on)`.
    pub parammod: Option<ButtonSkin>,
    /// `envcp_faderbg` / `envcp_fader` (thumb).
    pub fader_bg: Option<SkinImage>,
    pub fader_thumb: Option<SkinImage>,
    /// `envcp_knob_stack` — when `.fadermode` forces a knob.
    pub knob: Option<KnobSkin>,
}

/// The ECP theme context.
#[derive(Clone, PartialEq, Debug)]
pub struct EnvcpTheme {
    pub layouts: Vec<EnvcpLayout>,
    pub skin: Option<EnvcpSkin>,
}

impl EnvcpTheme {
    pub fn fts_default() -> Self {
        Self {
            layouts: vec![EnvcpLayout::fts_default()],
            skin: None,
        }
    }

    /// Resolve a layout by name; unknown/`None` falls back to the first.
    pub fn layout(&self, name: Option<&str>) -> &EnvcpLayout {
        name.and_then(|n| self.layouts.iter().find(|l| l.name == n))
            .or(self.layouts.first())
            .expect("EnvcpTheme has at least one layout")
    }
}

impl Default for EnvcpTheme {
    fn default() -> Self {
        Self::fts_default()
    }
}
