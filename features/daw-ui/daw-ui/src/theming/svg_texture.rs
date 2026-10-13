//! SVG texture support for custom control appearances.
//!
//! SVG textures allow controls to use custom graphics, such as
//! realistic knob images or sprite sheets for animated controls.

/// SVG texture for custom control appearance.
///
/// Supports two rendering modes:
/// - **Single image with rotation**: The SVG is rotated based on the value.
/// - **Sprite sheet**: Multiple frames arranged vertically, viewport is translated.
///
/// # Example
///
/// ```
/// use audio_controls::theming::SvgTexture;
///
/// // Single SVG that rotates
/// let rotating_knob = SvgTexture::rotating("<svg>...</svg>");
///
/// // Sprite sheet with 128 frames
/// let sprite_knob = SvgTexture::sprite_sheet("<svg>...</svg>", 128);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct SvgTexture {
    /// SVG content as a string.
    pub svg_content: String,
    /// Number of frames (1 for rotating, >1 for sprite sheet).
    pub frame_count: u32,
    /// Whether to use rotation (true) or sprite sheet (false).
    pub use_rotation: bool,
    /// Rotation range in degrees (for rotating mode).
    pub rotation_range: f32,
    /// Starting rotation offset in degrees.
    pub rotation_offset: f32,
}

impl SvgTexture {
    /// Create a rotating SVG texture.
    ///
    /// The SVG will be rotated from -135 to +135 degrees (270 degree sweep)
    /// based on the normalized value.
    #[must_use]
    pub fn rotating(svg_content: impl Into<String>) -> Self {
        Self {
            svg_content: svg_content.into(),
            frame_count: 1,
            use_rotation: true,
            rotation_range: 270.0,
            rotation_offset: -135.0,
        }
    }

    /// Create a sprite sheet texture.
    ///
    /// The SVG should contain all frames arranged vertically.
    /// The viewport will be translated to show the appropriate frame.
    #[must_use]
    pub fn sprite_sheet(svg_content: impl Into<String>, frames: u32) -> Self {
        Self {
            svg_content: svg_content.into(),
            frame_count: frames.max(1),
            use_rotation: false,
            rotation_range: 0.0,
            rotation_offset: 0.0,
        }
    }

    /// Create with custom rotation range.
    #[must_use]
    pub fn with_rotation_range(mut self, range: f32, offset: f32) -> Self {
        self.rotation_range = range;
        self.rotation_offset = offset;
        self
    }

    /// Calculate the rotation angle for a given normalized value.
    #[must_use]
    pub fn rotation_angle(&self, normalized: f32) -> f32 {
        self.rotation_offset + (normalized.clamp(0.0, 1.0) * self.rotation_range)
    }

    /// Calculate the frame index for a given normalized value.
    #[must_use]
    pub fn frame_index(&self, normalized: f32) -> u32 {
        if self.frame_count <= 1 {
            return 0;
        }

        let normalized = normalized.clamp(0.0, 1.0);
        let frame = (normalized * (self.frame_count - 1) as f32).round() as u32;
        frame.min(self.frame_count - 1)
    }

    /// Calculate the vertical offset percentage for sprite sheet rendering.
    #[must_use]
    pub fn frame_offset_percent(&self, normalized: f32) -> f32 {
        if self.frame_count <= 1 {
            return 0.0;
        }

        let frame = self.frame_index(normalized);
        (frame as f32 / self.frame_count as f32) * 100.0
    }

    /// Generate inline style for sprite sheet rendering.
    #[must_use]
    pub fn sprite_style(&self, normalized: f32) -> String {
        let offset = self.frame_offset_percent(normalized);
        format!("transform: translateY(-{offset}%);")
    }

    /// Generate inline style for rotation rendering.
    #[must_use]
    pub fn rotation_style(&self, normalized: f32) -> String {
        let angle = self.rotation_angle(normalized);
        format!("transform: rotate({angle}deg);")
    }
}

impl Default for SvgTexture {
    fn default() -> Self {
        Self::rotating("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_angles() {
        let texture = SvgTexture::rotating("<svg/>");

        assert!((texture.rotation_angle(0.0) - (-135.0)).abs() < f32::EPSILON);
        assert!((texture.rotation_angle(0.5) - 0.0).abs() < f32::EPSILON);
        assert!((texture.rotation_angle(1.0) - 135.0).abs() < f32::EPSILON);
    }

    #[test]
    fn sprite_sheet_frames() {
        let texture = SvgTexture::sprite_sheet("<svg/>", 100);

        assert_eq!(texture.frame_index(0.0), 0);
        assert_eq!(texture.frame_index(0.5), 50);
        assert_eq!(texture.frame_index(1.0), 99);
    }

    #[test]
    fn sprite_offset_calculation() {
        let texture = SvgTexture::sprite_sheet("<svg/>", 10);

        assert!((texture.frame_offset_percent(0.0) - 0.0).abs() < f32::EPSILON);
        assert!((texture.frame_offset_percent(1.0) - 90.0).abs() < f32::EPSILON);
    }
}
