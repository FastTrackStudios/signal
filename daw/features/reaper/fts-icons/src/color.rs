use anyhow::{Result, bail};
use resvg::tiny_skia::Color;

/// Parse `#rgb`, `#rrggbb`, or `#rrggbbaa` (leading `#` optional).
pub fn parse(s: &str) -> Result<Color> {
    let hex = s.trim().trim_start_matches('#');
    let (r, g, b, a) = match hex.len() {
        3 => {
            let v: Vec<u8> = hex
                .chars()
                .map(|c| u8::from_str_radix(&c.to_string(), 16).map(|n| n * 17))
                .collect::<Result<_, _>>()?;
            (v[0], v[1], v[2], 255)
        }
        6 => (
            u8::from_str_radix(&hex[0..2], 16)?,
            u8::from_str_radix(&hex[2..4], 16)?,
            u8::from_str_radix(&hex[4..6], 16)?,
            255,
        ),
        8 => (
            u8::from_str_radix(&hex[0..2], 16)?,
            u8::from_str_radix(&hex[2..4], 16)?,
            u8::from_str_radix(&hex[4..6], 16)?,
            u8::from_str_radix(&hex[6..8], 16)?,
        ),
        _ => bail!("bad color {s:?} — expected #rgb, #rrggbb, or #rrggbbaa"),
    };
    Ok(Color::from_rgba8(r, g, b, a))
}

/// Opaque rgb hex (alpha handled separately at composite time).
pub fn to_rgb_hex(c: Color) -> String {
    let c = c.to_color_u8();
    format!("#{:02x}{:02x}{:02x}", c.red(), c.green(), c.blue())
}
