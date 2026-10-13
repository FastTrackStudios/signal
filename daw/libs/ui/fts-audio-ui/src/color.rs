//! Minimal color-space helpers — convert the `oklch(...)` strings the
//! architect-ui theme uses into concrete `#rrggbb` values that blitz will
//! accept as SVG `stroke="..."` / `fill="..."` attributes.
//!
//! blitz's SVG layer doesn't honor `style="stroke: var(--…)"` on path
//! elements, so theme-reactive widgets that draw with SVG (knobs,
//! sliders, etc.) need to resolve their colors in Rust and emit hex.
//!
//! Falls through verbatim when the input is already a `#hex` /
//! `rgb(...)` / named color.

/// Parse a CSS color value the architect-ui theme system might emit and
/// return it as a `#rrggbb` string. Pass-through for inputs that aren't
/// `oklch(...)`. Returns `None` if parsing fails.
///
/// Supports:
/// - `oklch(L C h)` and `oklch(L C h / a)` (alpha ignored)
/// - `#hex` (passed through as-is)
/// - `rgb(r, g, b)` / `rgb(r g b)` (converted)
/// - bare hex without `#` (assumed `#`)
pub fn css_color_to_hex(value: &str) -> Option<String> {
    let v = value.trim();
    if v.starts_with('#') {
        return Some(v.to_string());
    }
    if let Some(rest) = v.strip_prefix("oklch(").and_then(|s| s.strip_suffix(')')) {
        return oklch_string_to_hex(rest);
    }
    if let Some(rest) = v.strip_prefix("rgb(").and_then(|s| s.strip_suffix(')')) {
        let parts: Vec<f32> = rest
            .replace(',', " ")
            .split_whitespace()
            .filter_map(|p| p.parse().ok())
            .collect();
        if parts.len() >= 3 {
            return Some(rgb_to_hex(parts[0], parts[1], parts[2]));
        }
    }
    None
}

/// Lookup helper that returns a hex color for a theme token, falling
/// back to `default_hex` if the token is missing or unparseable.
pub fn theme_token_hex<'a>(
    style: &fts_audio_ui_theme_lookup::Style<'a>,
    key: &str,
    default_hex: &str,
) -> String {
    style
        .get(key)
        .and_then(css_color_to_hex)
        .unwrap_or_else(|| default_hex.to_string())
}

fn oklch_string_to_hex(body: &str) -> Option<String> {
    // Strip any "/ alpha" tail.
    let no_alpha = body.split('/').next()?;
    // Tokens may be space-separated. `oklch(0.5 0.1 30)` /
    // `oklch(50% 0.1 30deg)` — strip trailing units, keep numbers.
    let tokens: Vec<&str> = no_alpha.split_whitespace().collect();
    if tokens.len() < 3 {
        return None;
    }
    let l = parse_number(tokens[0])?; // 0..1 (or 0..100 if a percent)
    let c = parse_number(tokens[1])?;
    let h_deg = parse_number(tokens[2])?;
    let l = if tokens[0].ends_with('%') {
        l / 100.0
    } else {
        l
    };
    let (r, g, b) = oklch_to_srgb(l, c, h_deg);
    Some(rgb_to_hex(r * 255.0, g * 255.0, b * 255.0))
}

fn parse_number(s: &str) -> Option<f32> {
    s.trim_end_matches(|c: char| !c.is_ascii_digit() && c != '.' && c != '-' && c != '+')
        .parse()
        .ok()
}

fn rgb_to_hex(r: f32, g: f32, b: f32) -> String {
    let r = r.clamp(0.0, 255.0).round() as u8;
    let g = g.clamp(0.0, 255.0).round() as u8;
    let b = b.clamp(0.0, 255.0).round() as u8;
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// oklch → linear sRGB via oklab, then gamma-encoded sRGB.
///
/// Reference: <https://bottosson.github.io/posts/oklab/>.
fn oklch_to_srgb(l: f32, c: f32, h_deg: f32) -> (f32, f32, f32) {
    let h = h_deg.to_radians();
    let a = c * h.cos();
    let b = c * h.sin();
    oklab_to_srgb(l, a, b)
}

fn oklab_to_srgb(l: f32, a: f32, b: f32) -> (f32, f32, f32) {
    // oklab → LMS (cube)
    let l_ = l + 0.396_337_8 * a + 0.215_803_76 * b;
    let m_ = l - 0.105_561_346 * a - 0.063_854_17 * b;
    let s_ = l - 0.089_484_18 * a - 1.291_485_5 * b;
    let l3 = l_ * l_ * l_;
    let m3 = m_ * m_ * m_;
    let s3 = s_ * s_ * s_;
    // LMS → linear sRGB
    let lr = 4.076_741_7 * l3 - 3.307_711_6 * m3 + 0.230_969_94 * s3;
    let lg = -1.268_438 * l3 + 2.609_757_4 * m3 - 0.341_319_38 * s3;
    let lb = -0.0041960863 * l3 - 0.703_418_6 * m3 + 1.707_614_7 * s3;
    (srgb_encode(lr), srgb_encode(lg), srgb_encode(lb))
}

fn srgb_encode(c: f32) -> f32 {
    if c <= 0.0031308 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

// Re-export ThemeStyle behind a small adapter so the knob doesn't need
// a hard dep on architect-ui (avoids a cycle). Consumers outside this crate
// who already have access to architect_ui::ThemeStyle can call
// `css_color_to_hex` directly.
mod fts_audio_ui_theme_lookup {
    /// Trait-shaped view of any theme style; matches the shape of
    /// architect-ui's `ThemeStyle::get`.
    pub struct Style<'a> {
        pub get: &'a dyn Fn(&str) -> Option<&'a str>,
    }

    impl<'a> Style<'a> {
        pub fn get(&self, key: &str) -> Option<&'a str> {
            (self.get)(key)
        }
    }
}

pub use fts_audio_ui_theme_lookup::Style as ThemeStyleView;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_through_hex() {
        assert_eq!(css_color_to_hex("#ff00aa").as_deref(), Some("#ff00aa"));
    }

    #[test]
    fn parses_oklch_pure_white() {
        // oklch(1 0 0) ≈ #ffffff
        let hex = css_color_to_hex("oklch(1 0 0)").unwrap();
        // Allow ±1 in any channel for rounding error.
        let r = u8::from_str_radix(&hex[1..3], 16).unwrap();
        let g = u8::from_str_radix(&hex[3..5], 16).unwrap();
        let b = u8::from_str_radix(&hex[5..7], 16).unwrap();
        assert!(r >= 254 && g >= 254 && b >= 254, "{hex}");
    }

    #[test]
    fn parses_oklch_pure_black() {
        let hex = css_color_to_hex("oklch(0 0 0)").unwrap();
        assert_eq!(hex.as_str(), "#000000");
    }

    #[test]
    fn parses_oklch_dark_grey() {
        // oklch(0.269 0 0) — architect-ui's dark border token.
        let hex = css_color_to_hex("oklch(0.269 0 0)").unwrap();
        // Should be a near-neutral mid-low grey.
        let r = u8::from_str_radix(&hex[1..3], 16).unwrap();
        let g = u8::from_str_radix(&hex[3..5], 16).unwrap();
        let b = u8::from_str_radix(&hex[5..7], 16).unwrap();
        assert!((r as i16 - g as i16).abs() <= 2);
        assert!((g as i16 - b as i16).abs() <= 2);
        assert!(
            r > 30 && r < 100,
            "expected mid-low grey for L=0.269, got {hex}"
        );
    }

    #[test]
    fn handles_oklch_with_alpha() {
        let hex = css_color_to_hex("oklch(0.5 0 0 / 0.5)").unwrap();
        assert!(hex.starts_with('#'));
    }

    #[test]
    fn returns_none_for_unparseable() {
        assert!(css_color_to_hex("blarg").is_none());
        assert!(css_color_to_hex("oklch()").is_none());
    }
}
