//! WALTER-compatible layout primitives.
//!
//! REAPER themes position every panel element with an 8-value coordinate list
//! `[x y w h ls ts rs bs]` — a px box plus per-edge *attach scales* that spring
//! each edge to the parent as it resizes (see
//! `reaper-theme/docs/walter-reference.md` §1). [`Coord`] is that model,
//! 1:1, so a REAPER-theme importer can copy coordinate lists straight into our
//! layouts. [`Margin`], [`FontSpec`] and [`ColorPair`] mirror the `*.margin`,
//! `*.font` and `*.color` attribute interpretations, and [`ThemeParam`] mirrors
//! `define_parameter` (theme-author-exposed knobs).

use super::theme::Color;

/// A WALTER coordinate list `[x y w h ls ts rs bs]`.
///
/// `x y w h` are px at the layout's *natural* size; `ls ts rs bs` attach each
/// edge to the parent's growth (`0` = pinned, `1` = follows the parent edge,
/// fractions/negatives allowed). A zero-sized box (`w == 0 && h == 0`) means
/// **hidden** — the WALTER idiom for removing an element from a layout.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Coord {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Left-edge attach scale (× parent width delta).
    pub ls: f32,
    /// Top-edge attach scale (× parent height delta).
    pub ts: f32,
    /// Right-edge attach scale (× parent width delta).
    pub rs: f32,
    /// Bottom-edge attach scale (× parent height delta).
    pub bs: f32,
}

impl Coord {
    /// A fixed px box pinned top-left (all attach scales 0).
    pub const fn px(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            x,
            y,
            w,
            h,
            ls: 0.0,
            ts: 0.0,
            rs: 0.0,
            bs: 0.0,
        }
    }

    /// The full 8-value form.
    #[allow(clippy::too_many_arguments)]
    pub const fn new(x: f32, y: f32, w: f32, h: f32, ls: f32, ts: f32, rs: f32, bs: f32) -> Self {
        Self {
            x,
            y,
            w,
            h,
            ls,
            ts,
            rs,
            bs,
        }
    }

    /// The hidden element (`[0]` in WALTER).
    pub const fn hidden() -> Self {
        Self::px(0.0, 0.0, 0.0, 0.0)
    }

    /// Whether this coordinate hides its element (zero-sized box).
    pub fn is_hidden(&self) -> bool {
        self.w <= 0.0 && self.h <= 0.0
    }

    /// Resolve to a concrete px rect for an actual parent size, given the
    /// layout's natural size. Per WALTER, each edge offset moves by its attach
    /// scale × the parent's growth delta:
    ///
    /// ```text
    /// left   = x       + ls·(W − W₀)        top    = y       + ts·(H − H₀)
    /// right  = (x + w) + rs·(W − W₀)        bottom = (y + h) + bs·(H − H₀)
    /// ```
    pub fn resolve(&self, natural: (f32, f32), parent: (f32, f32)) -> Rect {
        let dw = parent.0 - natural.0;
        let dh = parent.1 - natural.1;
        let left = self.x + self.ls * dw;
        let top = self.y + self.ts * dh;
        let right = self.x + self.w + self.rs * dw;
        let bottom = self.y + self.h + self.bs * dh;
        Rect {
            x: left,
            y: top,
            w: (right - left).max(0.0),
            h: (bottom - top).max(0.0),
        }
    }

    /// CSS `position:absolute` placement implementing the same edge math
    /// without measuring the parent: each component becomes
    /// `calc(<attach>·100% + <px offset>)`.
    ///
    /// Derivation (width shown; height analogous):
    /// `width = w + (rs−ls)·(W − W₀) = (rs−ls)·100% + (w − (rs−ls)·W₀)px`.
    pub fn css_position(&self, natural: (f32, f32)) -> String {
        let term = |scale: f32, px: f32| -> String {
            // Coerce non-finite components to 0: a runtime WALTER layout can
            // evaluate a coord to NaN/∞ at degenerate sizes, and emitting
            // `left:NaNpx` makes blitz hand vello a NaN path ("A path contains
            // NaN, ignoring it"). A 0 placement is harmless; a NaN poisons the
            // whole element's geometry.
            let scale = if scale.is_finite() { scale } else { 0.0 };
            let px = if px.is_finite() { px } else { 0.0 };
            if scale == 0.0 {
                format!("{px:.1}px")
            } else {
                format!("calc({:.4}% + {:.1}px)", scale * 100.0, px)
            }
        };
        let left = term(self.ls, self.x - self.ls * natural.0);
        let top = term(self.ts, self.y - self.ts * natural.1);
        let wscale = self.rs - self.ls;
        let hscale = self.bs - self.ts;
        let width = term(wscale, self.w - wscale * natural.0);
        let height = term(hscale, self.h - hscale * natural.1);
        format!("position:absolute; left:{left}; top:{top}; width:{width}; height:{height};")
    }
}

/// A resolved px rectangle.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// WALTER `*.margin` — `[left top right bottom justification]`;
/// justification `0` = left, `0.5` = center, `1` = right.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Margin {
    pub l: f32,
    pub t: f32,
    pub r: f32,
    pub b: f32,
    pub justify: f32,
}

impl Margin {
    pub const fn new(l: f32, t: f32, r: f32, b: f32, justify: f32) -> Self {
        Self {
            l,
            t,
            r,
            b,
            justify,
        }
    }

    /// CSS `text-align` for the justification scalar.
    pub fn text_align(&self) -> &'static str {
        if self.justify >= 0.75 {
            "right"
        } else if self.justify >= 0.25 {
            "center"
        } else {
            "left"
        }
    }

    /// CSS `padding` shorthand.
    pub fn css_padding(&self) -> String {
        format!(
            "{:.0}px {:.0}px {:.0}px {:.0}px",
            self.t, self.r, self.b, self.l
        )
    }
}

impl Default for Margin {
    fn default() -> Self {
        Self::new(0.0, 0.0, 0.0, 0.0, 0.5)
    }
}

/// WALTER `*.font` distilled to the vector-first essentials (the packed
/// REAPER form also carries row metrics; an importer maps what applies).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct FontSpec {
    pub size: f32,
    /// CSS font-weight (400 normal … 800 heavy).
    pub weight: u16,
}

impl FontSpec {
    pub const fn new(size: f32, weight: u16) -> Self {
        Self { size, weight }
    }
}

impl Default for FontSpec {
    fn default() -> Self {
        Self::new(11.0, 600)
    }
}

/// WALTER `*.color` — RGBA foreground + optional RGBA background.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ColorPair {
    pub fg: Color,
    pub bg: Option<Color>,
}

impl ColorPair {
    pub const fn fg(fg: Color) -> Self {
        Self { fg, bg: None }
    }
    pub const fn new(fg: Color, bg: Color) -> Self {
        Self { fg, bg: Some(bg) }
    }
}

/// Fader orientation — WALTER `*.fadermode` (`mcp.volume.fadermode` etc.).
/// The SDK form is a scalar: `1` forces a knob, `-1` prevents one, `0` is
/// REAPER's default (we then read the orientation off the box shape).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum FaderMode {
    #[default]
    Vertical,
    Horizontal,
    /// `*.fadermode 1` — render a rotary knob (filmstrip when skinned).
    Knob,
}

/// A theme-author-exposed knob — WALTER `define_parameter`
/// (`"name" "description" default min max`). Real themes drive tint
/// strengths, hide/show toggles and size adjusters through these.
#[derive(Clone, PartialEq, Debug)]
pub struct ThemeParam {
    pub name: String,
    pub desc: String,
    pub value: f32,
    pub default: f32,
    pub min: f32,
    pub max: f32,
}

impl ThemeParam {
    pub fn new(name: &str, desc: &str, default: f32, min: f32, max: f32) -> Self {
        Self {
            name: name.to_string(),
            desc: desc.to_string(),
            value: default,
            default,
            min,
            max,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAT: (f32, f32) = (64.0, 300.0);

    #[test]
    fn pinned_box_ignores_resize() {
        let c = Coord::px(6.0, 50.0, 25.0, 22.0);
        let r = c.resolve(NAT, (128.0, 600.0));
        assert_eq!((r.x, r.y, r.w, r.h), (6.0, 50.0, 25.0, 22.0));
    }

    #[test]
    fn bottom_pinned_tracks_height() {
        // Footer label: top + bottom edges fully attached to height.
        let c = Coord::new(0.0, 280.0, 64.0, 20.0, 0.0, 1.0, 1.0, 1.0);
        let r = c.resolve(NAT, (64.0, 400.0));
        assert_eq!((r.y, r.h), (380.0, 20.0));
        // Width follows the right edge.
        let r = c.resolve(NAT, (100.0, 300.0));
        assert_eq!((r.x, r.w), (0.0, 100.0));
    }

    #[test]
    fn stretching_body_grows_with_parent() {
        // Fader: bottom edge attached, top pinned.
        let c = Coord::new(6.0, 80.0, 30.0, 148.0, 0.0, 0.0, 0.0, 1.0);
        let r = c.resolve(NAT, (64.0, 500.0));
        assert_eq!((r.y, r.h), (80.0, 348.0));
    }

    #[test]
    fn hidden_is_zero_box() {
        assert!(Coord::hidden().is_hidden());
        assert!(!Coord::px(0.0, 0.0, 1.0, 1.0).is_hidden());
    }

    #[test]
    fn css_position_pinned_is_pure_px() {
        let css = Coord::px(6.0, 50.0, 25.0, 22.0).css_position(NAT);
        assert_eq!(
            css,
            "position:absolute; left:6.0px; top:50.0px; width:25.0px; height:22.0px;"
        );
    }

    #[test]
    fn css_position_attached_uses_calc() {
        // Bottom-right pinned 20×20 box.
        let c = Coord::new(44.0, 280.0, 20.0, 20.0, 1.0, 1.0, 1.0, 1.0);
        let css = c.css_position(NAT);
        // left tracks width 1:1 → calc(100% − 20px); size stays fixed.
        assert!(css.contains("left:calc(100.0000% + -20.0px)"), "{css}");
        assert!(css.contains("width:20.0px"), "{css}");
        assert!(css.contains("top:calc(100.0000% + -20.0px)"), "{css}");
        assert!(css.contains("height:20.0px"), "{css}");
    }

    #[test]
    fn margin_justify_maps_to_text_align() {
        assert_eq!(Margin::new(0.0, 0.0, 0.0, 0.0, 0.0).text_align(), "left");
        assert_eq!(Margin::new(0.0, 0.0, 0.0, 0.0, 0.5).text_align(), "center");
        assert_eq!(Margin::new(0.0, 0.0, 0.0, 0.0, 1.0).text_align(), "right");
    }
}
