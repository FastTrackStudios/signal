//! Layout form factors for audio control blocks.
//!
//! Defines standard sizes inspired by hardware rack equipment, guitar pedals, and plugin GUIs.

/// Style category for form factors (used for theming).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FormFactorCategory {
    /// Plugin-style GUI (clean, modern)
    Plugin,
    /// Rack-style (studio hardware aesthetic)
    Rack,
    /// Pedal-style (stompbox aesthetic with footswitch area)
    Pedal,
    /// Minimal (compact, no chrome)
    Minimal,
}

/// Standard form factors for block layouts.
///
/// These represent common aspect ratios and sizes used in:
/// - Plugin GUIs (Fullscreen, 16:9, 1:1)
/// - Hardware rack equipment (500-Series, 1U Rackspace)
/// - Guitar pedals (standard stompbox, mini, double-wide)
/// - Compact views (Mini)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FormFactor {
    /// Full plugin GUI - takes all available space
    Fullscreen,
    /// Widescreen panel (16:9 aspect ratio)
    Widescreen,
    /// Square panel (1:1 aspect ratio)
    #[default]
    Square,
    /// 500-Series module width (~1.5" / 38mm standard)
    /// Height is typically 5.25" / 133mm
    Series500,
    /// Double 500-Series width (~3" / 76mm)
    Series500Double,
    /// 1U Rack height (1.75" / 44.45mm), full width
    Rack1U,
    /// Half 1U Rack (half width)
    Rack1UHalf,
    /// Standard guitar pedal / stompbox (~2.5" x 4.5" / 64mm x 114mm)
    /// Classic Boss-style dimensions
    Pedal,
    /// Mini guitar pedal (~1.5" x 3.5" / 38mm x 89mm)
    /// Like MXR mini or TC Electronic mini
    PedalMini,
    /// Double-wide guitar pedal (~5" x 4.5" / 127mm x 114mm)
    /// Like Strymon big boxes or Line 6 HX Stomp
    PedalDouble,
    /// Minimal view - single control + bypass
    Mini,
}

impl FormFactor {
    /// Get the recommended aspect ratio for this form factor.
    /// Returns (width, height) ratio.
    #[must_use]
    pub const fn aspect_ratio(&self) -> (u32, u32) {
        match self {
            Self::Fullscreen => (16, 9), // Default, but adapts to container
            Self::Widescreen => (16, 9),
            Self::Square => (1, 1),
            Self::Series500 => (38, 133),       // ~0.286 W:H
            Self::Series500Double => (76, 133), // ~0.571 W:H
            Self::Rack1U => (482, 44),          // Standard 19" rack, ~10.95 W:H
            Self::Rack1UHalf => (241, 44),      // Half rack, ~5.48 W:H
            Self::Pedal => (64, 114),           // ~0.56 W:H (portrait orientation)
            Self::PedalMini => (38, 89),        // ~0.43 W:H (portrait, compact)
            Self::PedalDouble => (127, 114),    // ~1.11 W:H (landscape-ish)
            Self::Mini => (1, 1),               // Square, but small
        }
    }

    /// Get the recommended minimum width in pixels for this form factor.
    #[must_use]
    pub const fn min_width(&self) -> u32 {
        match self {
            Self::Fullscreen => 800,
            Self::Widescreen => 640,
            Self::Square => 200,
            Self::Series500 => 76,
            Self::Series500Double => 152,
            Self::Rack1U => 400,
            Self::Rack1UHalf => 200,
            Self::Pedal => 120,       // Standard stompbox
            Self::PedalMini => 80,    // Mini pedal
            Self::PedalDouble => 240, // Double-wide pedal
            Self::Mini => 48,
        }
    }

    /// Get the recommended minimum height in pixels for this form factor.
    #[must_use]
    pub const fn min_height(&self) -> u32 {
        match self {
            Self::Fullscreen => 450,
            Self::Widescreen => 360,
            Self::Square => 200,
            Self::Series500 => 266,
            Self::Series500Double => 266,
            Self::Rack1U => 44,
            Self::Rack1UHalf => 44,
            Self::Pedal => 200,       // Standard stompbox
            Self::PedalMini => 160,   // Mini pedal
            Self::PedalDouble => 200, // Double-wide pedal
            Self::Mini => 48,
        }
    }

    /// Get the Level of Detail appropriate for this form factor.
    #[must_use]
    pub const fn default_lod(&self) -> LevelOfDetail {
        match self {
            Self::Fullscreen | Self::Widescreen => LevelOfDetail::Full,
            Self::Square | Self::Series500Double | Self::PedalDouble => LevelOfDetail::Standard,
            Self::Series500 | Self::Rack1U | Self::Rack1UHalf | Self::Pedal => {
                LevelOfDetail::Compact
            }
            Self::PedalMini | Self::Mini => LevelOfDetail::Mini,
        }
    }

    /// Check if this is a pedal-style form factor.
    #[must_use]
    pub const fn is_pedal(&self) -> bool {
        matches!(self, Self::Pedal | Self::PedalMini | Self::PedalDouble)
    }

    /// Check if this is a rack-style form factor.
    #[must_use]
    pub const fn is_rack(&self) -> bool {
        matches!(
            self,
            Self::Series500 | Self::Series500Double | Self::Rack1U | Self::Rack1UHalf
        )
    }

    /// Get the style category for theming purposes.
    #[must_use]
    pub const fn style_category(&self) -> FormFactorCategory {
        match self {
            Self::Fullscreen | Self::Widescreen | Self::Square => FormFactorCategory::Plugin,
            Self::Series500 | Self::Series500Double | Self::Rack1U | Self::Rack1UHalf => {
                FormFactorCategory::Rack
            }
            Self::Pedal | Self::PedalMini | Self::PedalDouble => FormFactorCategory::Pedal,
            Self::Mini => FormFactorCategory::Minimal,
        }
    }

    /// Check if this form factor supports the given LOD.
    #[must_use]
    pub const fn supports_lod(&self, lod: LevelOfDetail) -> bool {
        let default = self.default_lod() as u8;
        let requested = lod as u8;
        // Can always render at lower detail than default
        requested >= default
    }
}

/// Level of Detail for block rendering.
///
/// Determines how much information and how many controls are displayed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
#[repr(u8)]
pub enum LevelOfDetail {
    /// Full detail - all parameters, visualizations, meters, labels
    Full = 0,
    /// Standard detail - essential parameters with labels
    #[default]
    Standard = 1,
    /// Compact detail - key parameters only, minimal labels
    Compact = 2,
    /// Mini detail - single macro control + bypass
    Mini = 3,
}

impl LevelOfDetail {
    /// Whether to show parameter labels at this LOD.
    #[must_use]
    pub const fn show_labels(&self) -> bool {
        matches!(self, Self::Full | Self::Standard)
    }

    /// Whether to show parameter values at this LOD.
    #[must_use]
    pub const fn show_values(&self) -> bool {
        matches!(self, Self::Full | Self::Standard | Self::Compact)
    }

    /// Whether to show visualizations (EQ curves, waveforms) at this LOD.
    #[must_use]
    pub const fn show_visualizations(&self) -> bool {
        matches!(self, Self::Full)
    }

    /// Whether to show meters at this LOD.
    #[must_use]
    pub const fn show_meters(&self) -> bool {
        matches!(self, Self::Full | Self::Standard)
    }

    /// Whether to use macro control (single knob) at this LOD.
    #[must_use]
    pub const fn use_macro_control(&self) -> bool {
        matches!(self, Self::Mini)
    }

    /// Get the recommended knob size for this LOD.
    #[must_use]
    pub const fn knob_size(&self) -> u32 {
        match self {
            Self::Full => 64,
            Self::Standard => 48,
            Self::Compact => 36,
            Self::Mini => 56, // Larger for single macro knob
        }
    }
}

/// Layout constraints for a block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutConstraints {
    /// The form factor being rendered into.
    pub form_factor: FormFactor,
    /// Available width in pixels.
    pub available_width: u32,
    /// Available height in pixels.
    pub available_height: u32,
    /// The LOD to use (may differ from form factor default).
    pub lod: LevelOfDetail,
}

impl LayoutConstraints {
    /// Create constraints from a form factor with default sizing.
    #[must_use]
    pub fn from_form_factor(form_factor: FormFactor) -> Self {
        Self {
            form_factor,
            available_width: form_factor.min_width(),
            available_height: form_factor.min_height(),
            lod: form_factor.default_lod(),
        }
    }

    /// Create constraints with custom dimensions.
    #[must_use]
    pub const fn new(form_factor: FormFactor, width: u32, height: u32, lod: LevelOfDetail) -> Self {
        Self {
            form_factor,
            available_width: width,
            available_height: height,
            lod,
        }
    }

    /// Automatically determine LOD based on available space.
    #[must_use]
    pub fn auto_lod(form_factor: FormFactor, width: u32, height: u32) -> Self {
        let lod = if width < 60 || height < 60 {
            LevelOfDetail::Mini
        } else if width < 120 || height < 80 {
            LevelOfDetail::Compact
        } else if width < 200 || height < 150 {
            LevelOfDetail::Standard
        } else {
            LevelOfDetail::Full
        };

        Self {
            form_factor,
            available_width: width,
            available_height: height,
            lod,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_factor_lod_compatibility() {
        // Mini form factor should only support Mini LOD
        assert!(FormFactor::Mini.supports_lod(LevelOfDetail::Mini));
        assert!(!FormFactor::Mini.supports_lod(LevelOfDetail::Full));

        // Fullscreen should support all LODs
        assert!(FormFactor::Fullscreen.supports_lod(LevelOfDetail::Full));
        assert!(FormFactor::Fullscreen.supports_lod(LevelOfDetail::Mini));
    }

    #[test]
    fn auto_lod_selection() {
        let small = LayoutConstraints::auto_lod(FormFactor::Square, 50, 50);
        assert_eq!(small.lod, LevelOfDetail::Mini);

        let large = LayoutConstraints::auto_lod(FormFactor::Square, 300, 300);
        assert_eq!(large.lod, LevelOfDetail::Full);
    }
}
