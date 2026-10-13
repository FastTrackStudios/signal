//! Maker's-mark lettering: the generic name set in the *style* of a unit's
//! logo — its weight, slant, tracking, colour and placement — never the
//! maker's name or model itself.

use frame::document::proto::{NodeId, NodeKind, Paint, TextAlign, TextRun};
use frame::surface::widgets::Builder;

use super::ui;

/// How a mark is set.
#[derive(Clone, Copy)]
pub struct Style {
    pub size: f64,
    /// 400 regular … 900 black. The one face the renderer carries has no
    /// heavy cut, so past 600 the mark is thickened by overprinting.
    pub weight: f64,
    pub italic: bool,
    /// Tracking, in ems (negative = tight, as heavy logotypes are set).
    pub tracking: f64,
}

/// Set `s` in `style`, its box `w` wide from `x`, vertically centred on `cy`.
/// Returns the first (topmost) text node.
#[allow(clippy::too_many_arguments)]
pub fn mark(
    b: &mut Builder<'_>,
    parent: NodeId,
    s: &str,
    x: f64,
    cy: f64,
    w: f64,
    style: Style,
    color: &str,
    align: TextAlign,
) -> NodeId {
    // How far the overprints spread either side, px: none at 600, about 4 %
    // of the size at 900.
    let spread = (style.size * 0.042 * (style.weight - 600.0) / 300.0).max(0.0);
    let offsets: Vec<(f64, f64)> = if spread > 0.2 {
        vec![
            (0.0, 0.0),
            (-spread, 0.0),
            (spread, 0.0),
            (-spread / 2.0, 0.0),
            (spread / 2.0, 0.0),
            (0.0, -spread / 2.0),
            (0.0, spread / 2.0),
        ]
    } else {
        vec![(0.0, 0.0)]
    };
    let mut first = None;
    for (dx, dy) in offsets {
        let mut n = ui::text(
            "mark",
            s,
            x + dx,
            cy + dy,
            w,
            style.size,
            color,
            align,
            style.weight,
            false,
        );
        if let NodeKind::Text(t) = &mut n.kind {
            t.letter_spacing = style.size * style.tracking;
            t.runs = vec![TextRun {
                text: s.to_string(),
                fills: vec![Paint::solid(frame::surface::kit::draw::hex(color))],
                font_family: None,
                font_size: Some(style.size),
                font_weight: Some(style.weight),
                font_style: style.italic.then(|| "Italic".to_string()),
                font_width: None,
                letter_spacing: Some(style.size * style.tracking),
            }];
        }
        let id = b.add(parent, n);
        first.get_or_insert(id);
    }
    first.unwrap_or(parent)
}
