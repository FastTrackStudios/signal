//! Parsing rendered layout back out of HTML — the shared half of the
//! panel tests.
//!
//! Twice during convergence a test broke on a refactor that changed
//! nothing real: the rail test named three viewBoxes and broke when bands
//! started drawing their own slices; the knob's unity test carried a
//! pasted coordinate and broke on a resize. The rewrites that survived
//! asked about *behaviour* — "the groove's slices sum to 27", "the boxes
//! on the column share an axis" — and this module is what makes that the
//! easy pattern: positioned boxes and SVG marks come back as numbers, so
//! an assertion says what it means instead of pattern-matching a style
//! string.
//!
//! Deliberately a parser of the rendered output and not a DOM walk: what
//! the tests guard is the exact text Blitz receives, and a coordinate
//! that moves shows up here as a number that moved.

// Each test binary compiles this module and uses its own subset of it.
#![allow(dead_code)]

/// A `position:absolute` box, read back from an inline style.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedBox {
    pub left: Option<f32>,
    pub top: Option<f32>,
    pub bottom: Option<f32>,
    pub width: Option<f32>,
    pub height: Option<f32>,
}

/// Every absolutely positioned box in the HTML, in document order.
pub fn abs_boxes(html: &str) -> Vec<PlacedBox> {
    html.split("style=\"")
        .skip(1)
        .filter_map(|rest| {
            let style = rest.split('"').next()?;
            if !style.contains("position:absolute") {
                return None;
            }
            Some(PlacedBox {
                left: style_px(style, "left"),
                top: style_px(style, "top"),
                bottom: style_px(style, "bottom"),
                width: style_px(style, "width"),
                height: style_px(style, "height"),
            })
        })
        .collect()
}

/// The tops of the boxes on one left edge, sorted top to bottom.
///
/// "The boxes on the column" is a question about an axis, not about a
/// style string — this is how the padding and spread tests read REAPER's
/// offset chain back out of the render.
pub fn tops_at(html: &str, left: f32) -> Vec<f32> {
    let mut out: Vec<f32> = abs_boxes(html)
        .into_iter()
        .filter(|b| b.left.is_some_and(|l| (l - left).abs() < 0.01))
        .filter_map(|b| b.top)
        .collect();
    out.sort_by(|a, b| a.partial_cmp(b).unwrap());
    out
}

/// One `name:<value>px` declaration out of a style string.
fn style_px(style: &str, name: &str) -> Option<f32> {
    // Matched as a declaration, not a substring: `top` alone would also
    // find `margin-top`.
    style.split(';').map(str::trim).find_map(|decl| {
        decl.strip_prefix(name)?
            .strip_prefix(':')?
            .strip_suffix("px")?
            .parse()
            .ok()
    })
}

/// A mark inside an SVG — a `<rect>` or `<text>`, by its attributes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SvgMark {
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub width: Option<f32>,
    pub height: Option<f32>,
}

/// Every `<rect>` in the HTML, in document order.
pub fn svg_rects(html: &str) -> Vec<SvgMark> {
    svg_marks(html, "<rect ")
}

/// The x anchor of every `<text>` in the HTML, in document order.
pub fn svg_text_anchors(html: &str) -> Vec<f32> {
    svg_marks(html, "<text ")
        .into_iter()
        .filter_map(|m| m.x)
        .collect()
}

fn svg_marks(html: &str, open: &str) -> Vec<SvgMark> {
    html.match_indices(open)
        .filter_map(|(i, _)| {
            let el = html[i..].split('>').next()?;
            Some(SvgMark {
                x: attr_num(el, "x"),
                y: attr_num(el, "y"),
                width: attr_num(el, "width"),
                height: attr_num(el, "height"),
            })
        })
        .collect()
}

/// One numeric attribute off an element's opening tag.
fn attr_num(el: &str, name: &str) -> Option<f32> {
    let probe = format!(" {name}=\"");
    let at = el.find(&probe)? + probe.len();
    el[at..].split('"').next()?.parse().ok()
}
