use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const WHITE: Self = Self::new(1.0, 1.0, 1.0, 1.0);
    pub const BLACK: Self = Self::new(0.0, 0.0, 0.0, 1.0);

    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub fn to_css_rgba(&self) -> String {
        let r = (self.r.clamp(0.0, 1.0) * 255.0).round();
        let g = (self.g.clamp(0.0, 1.0) * 255.0).round();
        let b = (self.b.clamp(0.0, 1.0) * 255.0).round();
        format!("rgba({r}, {g}, {b}, {})", self.a.clamp(0.0, 1.0))
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::WHITE
    }
}
