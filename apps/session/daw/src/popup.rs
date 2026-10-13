//! A list that pops up over the window: a column of choices, ticked where
//! one is on, one lit under the pointer — the arrangement's menus.
//!
//! Generic over what a choice does (`A`); the menu only lays the choices
//! out, lights one and says which was taken. A list longer than the window
//! is tall runs on in another column beside it rather than off the bottom.

use anyrender::PaintScene;
use vello::kurbo::{Affine, Rect, RoundedRect, Stroke};
use vello::peniko::Fill;

use crate::arrangement::Palette;
use crate::text::Font;

/// One choice's height.
const ROW_H: f64 = 20.0;
/// The type size.
const SIZE: f32 = 11.0;
/// The room inside the edge.
const INSET: f64 = 4.0;

/// One choice.
#[derive(Clone, Debug)]
pub struct Choice<A> {
    pub label: String,
    /// Ticked: it is on.
    pub ticked: bool,
    /// It opens another list (drawn with an arrow).
    pub more: bool,
    /// A heading: drawn dim, not taken.
    pub heading: bool,
    pub action: Option<A>,
}

impl<A> Choice<A> {
    /// A choice that does `action`.
    pub fn new(label: impl Into<String>, ticked: bool, action: A) -> Self {
        Self {
            label: label.into(),
            ticked,
            more: false,
            heading: false,
            action: Some(action),
        }
    }

    /// A choice that opens another list.
    pub fn more(label: impl Into<String>, action: A) -> Self {
        Self {
            more: true,
            ..Self::new(label, false, action)
        }
    }

    /// A heading over the choices after it.
    pub fn heading(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            ticked: false,
            more: false,
            heading: true,
            action: None,
        }
    }
}

/// A list open on screen.
#[derive(Clone, Debug)]
pub struct Popup<A> {
    pub choices: Vec<Choice<A>>,
    /// Its top left.
    at: (f64, f64),
    column_w: f64,
    per_column: usize,
    pub hover: Option<usize>,
}

impl<A: Clone> Popup<A> {
    /// The list opened at `(x, y)`, kept inside `(width, height)`.
    #[must_use]
    pub fn open(
        font: &Font,
        choices: Vec<Choice<A>>,
        (x, y): (f64, f64),
        (width, height): (f64, f64),
    ) -> Self {
        let widest = choices
            .iter()
            .map(|c| font.width(&c.label, SIZE))
            .fold(80.0_f64, f64::max);
        let column_w = (widest + 2.0f64.mul_add(INSET, 34.0)).min(320.0);
        let fits = ((2.0f64.mul_add(-INSET, height) - 8.0) / ROW_H)
            .floor()
            .max(4.0);
        let per_column = num_traits::ToPrimitive::to_usize(&fits).unwrap_or(4).max(1);
        let columns = choices.len().div_ceil(per_column).max(1);
        let rows = choices.len().min(per_column);
        let w = column_w * f64::from(u32::try_from(columns).unwrap_or(1));
        let h = ROW_H.mul_add(f64::from(u32::try_from(rows).unwrap_or(1)), 2.0 * INSET);
        let at = (
            x.min(width - w - 4.0).max(4.0),
            y.min(height - h - 4.0).max(4.0),
        );
        Self {
            choices,
            at,
            column_w,
            per_column,
            hover: None,
        }
    }

    /// The whole list's box.
    #[must_use]
    pub fn bounds(&self) -> Rect {
        let columns = self.choices.len().div_ceil(self.per_column).max(1);
        let rows = self.choices.len().min(self.per_column);
        Rect::new(
            self.at.0,
            self.at.1,
            self.column_w
                .mul_add(f64::from(u32::try_from(columns).unwrap_or(1)), self.at.0),
            ROW_H.mul_add(
                f64::from(u32::try_from(rows).unwrap_or(1)),
                2.0f64.mul_add(INSET, self.at.1),
            ),
        )
    }

    fn row_rect(&self, index: usize) -> Rect {
        let column = index.checked_div(self.per_column).unwrap_or(0);
        let row = index.checked_rem(self.per_column).unwrap_or(index);
        let x = self
            .column_w
            .mul_add(f64::from(u32::try_from(column).unwrap_or(0)), self.at.0);
        let y = ROW_H.mul_add(
            f64::from(u32::try_from(row).unwrap_or(0)),
            self.at.1 + INSET,
        );
        Rect::new(x, y, x + self.column_w, y + ROW_H)
    }

    /// The choice under a point, if one can be taken.
    #[must_use]
    pub fn at(&self, x: f64, y: f64) -> Option<usize> {
        (0..self.choices.len())
            .find(|&i| {
                let r = self.row_rect(i);
                x >= r.x0 && x < r.x1 && y >= r.y0 && y < r.y1
            })
            .filter(|&i| self.choices.get(i).is_some_and(|c| !c.heading))
    }

    /// Light the choice under a point; whether that changed what is lit.
    pub fn hover_at(&mut self, x: f64, y: f64) -> bool {
        let now = self.at(x, y);
        let changed = now != self.hover;
        self.hover = now;
        changed
    }

    /// What the choice under a point does, if there is one.
    #[must_use]
    pub fn take(&self, x: f64, y: f64) -> Option<A> {
        self.at(x, y)
            .and_then(|i| self.choices.get(i))
            .and_then(|c| c.action.clone())
    }

    /// Draw it.
    pub fn paint(&self, painter: &mut impl PaintScene, palette: &Palette, font: &Font) {
        let bounds = self.bounds();
        let shape = RoundedRect::from_rect(bounds, 5.0);
        painter.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            palette.surface,
            None,
            &shape,
        );
        painter.stroke(
            &Stroke::new(1.0),
            Affine::IDENTITY,
            palette.tcp_rule,
            None,
            &shape,
        );
        for (i, choice) in self.choices.iter().enumerate() {
            let r = self.row_rect(i);
            if self.hover == Some(i) {
                painter.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    palette.accent.multiply_alpha(0.35),
                    None,
                    &RoundedRect::from_rect(r.inset(-2.0), 3.0),
                );
            }
            let ink = if choice.heading {
                palette.text_faint
            } else {
                palette.text
            };
            let baseline = r.y0 + 14.0;
            if choice.ticked {
                // A tick, drawn: the font need not have one.
                let (x, y) = (r.x0 + INSET + 3.0, r.y0 + 10.0);
                let mut tick = vello::kurbo::BezPath::new();
                tick.move_to((x, y));
                tick.line_to((x + 3.0, y + 3.5));
                tick.line_to((x + 9.0, y - 4.0));
                painter.stroke(
                    &Stroke::new(1.6),
                    Affine::IDENTITY,
                    palette.accent,
                    None,
                    &tick,
                );
            }
            let room = r.width() - 34.0;
            let label = font.elide(&choice.label, SIZE, room);
            crate::tcp::glyphs(
                painter,
                font,
                ink,
                &label,
                r.x0 + INSET + 16.0,
                baseline,
                SIZE,
            );
            if choice.more {
                let (x, y) = (r.x1 - 12.0, r.y0 + 10.0);
                let mut arrow = vello::kurbo::BezPath::new();
                arrow.move_to((x, y - 4.0));
                arrow.line_to((x + 4.0, y));
                arrow.line_to((x, y + 4.0));
                painter.stroke(
                    &Stroke::new(1.3),
                    Affine::IDENTITY,
                    palette.text_dim,
                    None,
                    &arrow,
                );
            }
        }
    }
}
