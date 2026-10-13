//! One theme for every generated control: semantic color roles, a size
//! scale and spacing — so a whole surface is consistent by construction
//! and a restyle is one value change, never a hunt through hex literals.

use document_proto::Color;

/// The control size scale. Sizes are the control's primary dimension
/// (knob diameter, footswitch edge) in px at 1× scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum ControlSize {
    Small,
    #[default]
    Medium,
    Large,
    /// Stage use: footswitch tiles, big transport buttons.
    Stage,
}

impl ControlSize {
    #[must_use]
    pub const fn px(self) -> f64 {
        match self {
            Self::Small => 40.0,
            Self::Medium => 56.0,
            Self::Large => 72.0,
            Self::Stage => 112.0,
        }
    }
}

const fn hex(rgb: u32) -> Color {
    Color {
        r: ((rgb >> 16) & 0xff) as f64 / 255.0,
        g: ((rgb >> 8) & 0xff) as f64 / 255.0,
        b: (rgb & 0xff) as f64 / 255.0,
        a: 1.0,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub name: &'static str,
    /// Surface background.
    pub background: Color,
    /// Panel / group background.
    pub panel: Color,
    /// Control bodies (knob caps, button faces).
    pub body: Color,
    /// Recessed tracks (knob track arc, fader slot, meter background).
    pub track: Color,
    /// Value indication (arcs, fills).
    pub accent: Color,
    /// Pointer / thumb.
    pub pointer: Color,
    /// Primary text.
    pub text: Color,
    /// Secondary text (labels).
    pub text_dim: Color,
    /// LEDs when on.
    pub led: Color,
    /// Meter segments: nominal, hot, clip.
    pub meter: Color,
    pub meter_hot: Color,
    pub meter_clip: Color,
    /// Corner radius for panels and buttons.
    pub radius: f64,
    /// Gap between controls.
    pub gap: f64,
    /// Panel padding.
    pub padding: f64,
    /// Label font size.
    pub label_size: f64,
    /// Readout font size.
    pub value_size: f64,
    /// Smallest allowed touch target (lint uses it too).
    pub min_touch: f64,
}

impl Theme {
    /// Dark, high-contrast, made for dim stages.
    #[must_use]
    pub const fn stage_dark() -> Self {
        Self {
            name: "stage-dark",
            background: hex(0x0e_0f_12),
            panel: hex(0x17_19_1e),
            body: hex(0x26_29_30),
            track: hex(0x0a_0b_0d),
            accent: hex(0xff_7a_1a),
            pointer: hex(0xf2_f2_f2),
            text: hex(0xf2_f2_f2),
            text_dim: hex(0x9a_a0_ab),
            led: hex(0x3d_ff_7a),
            meter: hex(0x3d_d6_7a),
            meter_hot: hex(0xf5_c4_2c),
            meter_clip: hex(0xff_3b_30),
            radius: 10.0,
            gap: 12.0,
            padding: 16.0,
            label_size: 11.0,
            value_size: 12.0,
            min_touch: 44.0,
        }
    }

    /// For bright rooms and daylight gigs.
    #[must_use]
    pub const fn daylight() -> Self {
        Self {
            name: "daylight",
            background: hex(0xf4_f4_f1),
            panel: hex(0xff_ff_ff),
            body: hex(0xe3_e3_de),
            track: hex(0xc9_c9_c2),
            accent: hex(0xd9_4a_00),
            pointer: hex(0x1a_1a_1a),
            text: hex(0x14_14_14),
            text_dim: hex(0x5a_5a_55),
            led: hex(0x00_a8_45),
            meter: hex(0x00_a8_45),
            meter_hot: hex(0xd1_9a_00),
            meter_clip: hex(0xd4_1c_12),
            radius: 10.0,
            gap: 12.0,
            padding: 16.0,
            label_size: 11.0,
            value_size: 12.0,
            min_touch: 44.0,
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::stage_dark()
    }
}
