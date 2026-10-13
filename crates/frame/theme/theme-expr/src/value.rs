//! What an expression evaluates to.

use std::fmt;
use std::sync::Arc;

/// A colour: sRGB channels and straight alpha, each 0..1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    pub const BLACK: Rgba = Rgba {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    pub const WHITE: Rgba = Rgba {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };

    #[must_use]
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa` (the `#` optional).
    #[must_use]
    pub fn from_hex(hex: &str) -> Option<Self> {
        let h = hex.strip_prefix('#').unwrap_or(hex);
        if !h.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let digit = |i: usize| u8::from_str_radix(h.get(i..=i)?, 16).ok();
        let pair = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
        let (r, g, b, a) = match h.len() {
            3 | 4 => {
                let one = |i| digit(i).map(|d| d * 17);
                (
                    one(0)?,
                    one(1)?,
                    one(2)?,
                    if h.len() == 4 { one(3)? } else { 255 },
                )
            }
            6 | 8 => (
                pair(0)?,
                pair(2)?,
                pair(4)?,
                if h.len() == 8 { pair(6)? } else { 255 },
            ),
            _ => return None,
        };
        Some(Self::from_rgba8(r, g, b, a))
    }

    #[must_use]
    pub fn from_rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        let c = |v: u8| f32::from(v) / 255.0;
        Self {
            r: c(r),
            g: c(g),
            b: c(b),
            a: c(a),
        }
    }

    /// Channels as bytes, rounded.
    #[must_use]
    pub fn to_rgba8(self) -> [u8; 4] {
        // In 0..=255 after the clamp, so the cast is exact.
        let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        [c(self.r), c(self.g), c(self.b), c(self.a)]
    }

    /// `#rrggbb`, or `#rrggbbaa` when not opaque.
    #[must_use]
    pub fn to_hex(self) -> String {
        let [r, g, b, a] = self.to_rgba8();
        if a == 255 {
            format!("#{r:02x}{g:02x}{b:02x}")
        } else {
            format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
        }
    }

    /// Linear interpolation, channel by channel (`t` clamped to 0..1).
    #[must_use]
    pub fn mix(self, other: Rgba, t: f64) -> Rgba {
        let t = t.clamp(0.0, 1.0) as f32;
        let l = |a: f32, b: f32| a + (b - a) * t;
        Rgba {
            r: l(self.r, other.r),
            g: l(self.g, other.g),
            b: l(self.b, other.b),
            a: l(self.a, other.a),
        }
    }

    /// Relative luminance (sRGB coefficients over the encoded channels —
    /// the cheap reading every theme tool uses to pick ink).
    #[must_use]
    pub fn luma(self) -> f64 {
        0.2126 * f64::from(self.r) + 0.7152 * f64::from(self.g) + 0.0722 * f64::from(self.b)
    }
}

/// A value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Nothing to show: the host has no such field, or no data for it yet.
    Missing,
    Num(f64),
    Bool(bool),
    Color(Rgba),
    Text(Arc<str>),
}

impl Value {
    #[must_use]
    pub fn text(s: &str) -> Self {
        Value::Text(Arc::from(s))
    }

    #[must_use]
    pub const fn is_missing(&self) -> bool {
        matches!(self, Value::Missing)
    }

    /// As a number: numbers, and booleans as 0/1.
    #[must_use]
    pub fn num(&self) -> Option<f64> {
        match self {
            Value::Num(n) => Some(*n),
            Value::Bool(b) => Some(f64::from(u8::from(*b))),
            _ => None,
        }
    }

    /// As a condition: booleans, and numbers as non-zero.
    #[must_use]
    pub fn truthy(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            Value::Num(n) => Some(*n != 0.0),
            Value::Text(t) => Some(!t.is_empty()),
            _ => None,
        }
    }

    #[must_use]
    pub fn color(&self) -> Option<Rgba> {
        match self {
            Value::Color(c) => Some(*c),
            _ => None,
        }
    }

    /// As text: text as is, numbers and booleans written out.
    #[must_use]
    pub fn as_text(&self) -> Option<Arc<str>> {
        match self {
            Value::Text(t) => Some(t.clone()),
            Value::Num(_) | Value::Bool(_) => Some(Arc::from(self.to_string())),
            _ => None,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Missing => f.write_str(""),
            Value::Num(n) => write!(f, "{n}"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Color(c) => f.write_str(&c.to_hex()),
            Value::Text(t) => f.write_str(t),
        }
    }
}

impl From<f64> for Value {
    fn from(n: f64) -> Self {
        Value::Num(n)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}

impl From<Rgba> for Value {
    fn from(c: Rgba) -> Self {
        Value::Color(c)
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::text(s)
    }
}

impl From<Arc<str>> for Value {
    fn from(s: Arc<str>) -> Self {
        Value::Text(s)
    }
}
