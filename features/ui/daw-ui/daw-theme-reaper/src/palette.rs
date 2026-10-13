//! `.ReaperTheme` ini parsing — the `[color theme]` palette.
//!
//! Values are signed decimal Windows `COLORREF`s: `0x00BBGGRR` (red in the low
//! byte). Some keys are *not* colors (blend modes like `*_drawmode`, flag
//! words); the raw integer map is kept so a mapper can consult anything, and
//! [`Palette::color`] decodes the COLORREF interpretation.

use std::collections::HashMap;

/// A decoded RGBA color (importer-local; the UI maps it onto its own type).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    /// Decode a Windows `COLORREF` (`0x00BBGGRR`, red low byte), opaque.
    pub fn from_colorref(v: i32) -> Self {
        let v = v as u32;
        Self {
            r: (v & 0xff) as u8,
            g: ((v >> 8) & 0xff) as u8,
            b: ((v >> 16) & 0xff) as u8,
            a: 0xff,
        }
    }

    /// Decode the rtconfig `AABBGGRR` hex form (zero-line colors etc.).
    pub fn from_aabbggrr(v: u32) -> Self {
        Self {
            r: (v & 0xff) as u8,
            g: ((v >> 8) & 0xff) as u8,
            b: ((v >> 16) & 0xff) as u8,
            a: ((v >> 24) & 0xff) as u8,
        }
    }
}

/// A decoded `*_drawmode` blend word — how a paired color composites over
/// what's beneath it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Drawmode {
    /// 0 normal, 1 additive, 2 dodge, 3 multiply, 4 overlay, 254 HSV adjust.
    pub blend: u8,
    /// 0.0–1.0 (clamp for safety; the encoding allows out-of-range words).
    pub alpha: f32,
}

/// The `[color theme]` section: raw ints by key.
#[derive(Clone, Debug, Default)]
pub struct Palette {
    raw: HashMap<String, i32>,
}

impl Palette {
    /// Parse the `[color theme]` section out of a `.ReaperTheme` ini.
    pub fn parse(ini: &str) -> Self {
        let mut raw = HashMap::new();
        let mut in_colors = false;
        for line in ini.lines() {
            let line = line.trim();
            if let Some(section) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                in_colors = section.eq_ignore_ascii_case("color theme");
                continue;
            }
            if !in_colors {
                continue;
            }
            if let Some((key, value)) = line.split_once('=')
                && let Ok(v) = value.trim().parse::<i32>()
            {
                raw.insert(key.trim().to_string(), v);
            }
        }
        Self { raw }
    }

    /// Raw integer for a key (drawmodes, flags, colors alike).
    pub fn int(&self, key: &str) -> Option<i32> {
        self.raw.get(key).copied()
    }

    /// COLORREF-decoded color for a key.
    pub fn color(&self, key: &str) -> Option<Rgba> {
        self.int(key).map(Rgba::from_colorref)
    }

    /// Decoded `*_drawmode` / `*dm` blend word for a key (per
    /// `ReaperTheme_Combiner.lua`): low byte = blend mode (0 normal, 1 add,
    /// 2 dodge, 3 multiply, 4 overlay, 254 HSV adjust), alpha =
    /// `(((n >> 8) & 0x3ff) - 0x200) / 256`.
    pub fn drawmode(&self, key: &str) -> Option<Drawmode> {
        let n = self.int(key)? as u32;
        Some(Drawmode {
            blend: (n & 0xff) as u8,
            alpha: ((((n >> 8) & 0x3ff) as f32) - 512.0) / 256.0,
        })
    }

    /// Number of parsed entries.
    pub fn len(&self) -> usize {
        self.raw.len()
    }
    pub fn is_empty(&self) -> bool {
        self.raw.is_empty()
    }

    /// Iterate raw entries.
    pub fn iter(&self) -> impl Iterator<Item = (&str, i32)> {
        self.raw.iter().map(|(k, v)| (k.as_str(), *v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INI: &str = "\
[REAPER]
ui_img=MyTheme
[color theme]
col_main_bg2=2festering
col_arrangebg=4539717
col_tr1_bg=7697781
marquee_drawmode=168
";

    #[test]
    fn parses_color_theme_section_only() {
        let p = Palette::parse(INI);
        assert_eq!(p.int("col_arrangebg"), Some(4539717));
        assert_eq!(p.int("marquee_drawmode"), Some(168));
        // Malformed value line skipped, [REAPER] section ignored.
        assert_eq!(p.int("col_main_bg2"), None);
        assert_eq!(p.int("ui_img"), None);
        assert_eq!(p.len(), 3);
    }

    #[test]
    fn colorref_decodes_red_low_byte() {
        // 4539717 = 0x45463F45? compute: 4539717 = 0x454545 → r=g=b=0x45.
        assert_eq!(
            Rgba::from_colorref(0x454545),
            Rgba {
                r: 0x45,
                g: 0x45,
                b: 0x45,
                a: 0xff
            }
        );
        // Distinct channels: 0x00BBGGRR.
        assert_eq!(
            Rgba::from_colorref(0x00102030),
            Rgba {
                r: 0x30,
                g: 0x20,
                b: 0x10,
                a: 0xff
            }
        );
    }

    #[test]
    fn aabbggrr_decodes_alpha_high_byte() {
        assert_eq!(
            Rgba::from_aabbggrr(0xFF666666),
            Rgba {
                r: 0x66,
                g: 0x66,
                b: 0x66,
                a: 0xff
            }
        );
        assert_eq!(
            Rgba::from_aabbggrr(0x85000000),
            Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 0x85
            }
        );
    }
}
