//! The movement's geometry and the scale law — ported from
//! `fts-audio-ui::hardware::vu_svg`.
//!
//! Everything here is in the card's design space, a `100 × 58` box (the
//! source's SVG `viewBox`); the builder multiplies by the placed size. The
//! pivot sits well below the card with a long needle, so the scale is the
//! wide, shallow arc of a real movement rather than a steep rainbow.
//!
//! **The scale law.** A VU card is not linear in dB: `-20 … +3` with the
//! bottom crowded and 0 VU at ~70 % of the travel. The source prints it as
//! `fraction = t^1.9` where `t` is the linear position of the value in
//! `-20..+3`. The runtime's `Gauge` maps a level linearly onto `floor..ceiling`
//! and then through the binding's curve, so a VU card binds with
//! [`Curve::Power`] `{ exponent: 1.9 }` over `-20..+3` — the *same* function
//! the ticks are placed with, so the needle lands exactly on every printed
//! mark, not just near them. A decibel card is linear (`-40..+20`).

use surface_proto::Curve;

use crate::draw::polar;

/// Design-space width of the card.
pub const VU_W: f64 = 100.0;
/// Design-space height of the card.
pub const VU_H: f64 = 58.0;
/// The needle pivot, below the card.
pub const PIVOT_X: f64 = VU_W * 0.5;
pub const PIVOT_Y: f64 = VU_H * 1.52;
/// Needle length from the pivot.
pub const NEEDLE_LEN: f64 = VU_H * 1.40;
/// Half the sweep, degrees either side of straight up.
pub const SWEEP_DEG: f64 = 34.0;
/// The VU card's law: `fraction = t^VU_EXPONENT`.
pub const VU_EXPONENT: f64 = 1.9;

/// What a movement's card is printed with.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Card {
    /// The VU standard, `-20 … +3`, crowded at the bottom, red above 0.
    Vu,
    /// A plain decibel readout, `-40 … +20`, evenly spaced, no red (the dbx's).
    Decibels,
}

/// The VU card's ticks: (value, label, major). Majors are numbered.
const VU_TICKS: &[(f64, &str, bool)] = &[
    (-20.0, "20", true),
    (-10.0, "10", true),
    (-7.0, "7", false),
    (-5.0, "5", true),
    (-3.0, "3", false),
    (-2.0, "2", false),
    (-1.0, "1", false),
    (0.0, "0", true),
    (1.0, "1", false),
    (2.0, "2", false),
    (3.0, "3", true),
];

/// The decibel card's ticks, every 5 dB, numbered every 10.
const DB_TICKS: &[(f64, &str, bool)] = &[
    (-40.0, "-40", true),
    (-35.0, "", false),
    (-30.0, "-30", true),
    (-25.0, "", false),
    (-20.0, "-20", true),
    (-15.0, "", false),
    (-10.0, "-10", true),
    (-5.0, "", false),
    (0.0, "0", true),
    (5.0, "", false),
    (10.0, "+10", true),
    (15.0, "", false),
    (20.0, "+20", true),
];

impl Card {
    /// The ticks printed on this card, as (value, label, major).
    #[must_use]
    pub const fn ticks(self) -> &'static [(f64, &'static str, bool)] {
        match self {
            Self::Vu => VU_TICKS,
            Self::Decibels => DB_TICKS,
        }
    }

    /// The printed range, left stop to right stop — the gauge's
    /// `floor_db..ceiling_db`.
    #[must_use]
    pub const fn range(self) -> (f64, f64) {
        match self {
            Self::Vu => (-20.0, 3.0),
            Self::Decibels => (-40.0, 20.0),
        }
    }

    /// The binding curve that reproduces this card's law over [`Self::range`].
    #[must_use]
    pub const fn curve(self) -> Curve {
        match self {
            Self::Vu => Curve::Power { exponent: VU_EXPONENT },
            Self::Decibels => Curve::Linear,
        }
    }

    /// Where `value` sits along the travel, 0 = left stop, 1 = right.
    #[must_use]
    pub fn fraction(self, value: f64) -> f64 {
        let (lo, hi) = self.range();
        self.curve().apply((value - lo) / (hi - lo))
    }

    /// The needle angle for `value`, degrees from straight up (clockwise).
    #[must_use]
    pub fn angle(self, value: f64) -> f64 {
        (self.fraction(value) * 2.0 - 1.0) * SWEEP_DEG
    }

    /// The stretch printed red (values above this), if any.
    #[must_use]
    pub const fn hot_from(self) -> Option<f64> {
        match self {
            Self::Vu => Some(0.0),
            Self::Decibels => None,
        }
    }

    /// The instrument the card prints under the scale by default.
    #[must_use]
    pub const fn legend(self) -> &'static str {
        match self {
            Self::Vu => "VU",
            Self::Decibels => "dB",
        }
    }
}

/// A point on the scale for `value`, `inset` in from the needle's reach.
#[must_use]
pub fn point(card: Card, value: f64, inset: f64) -> (f64, f64) {
    polar(PIVOT_X, PIVOT_Y, NEEDLE_LEN - inset, card.angle(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vu_card_crowds_its_bottom_and_puts_zero_near_three_quarters() {
        let c = Card::Vu;
        assert!(c.fraction(-10.0) - c.fraction(-20.0) < c.fraction(0.0) - c.fraction(-10.0));
        let zero = c.fraction(0.0);
        assert!((0.7..0.8).contains(&zero), "0 VU at {zero}");
        assert!((c.angle(-20.0) + SWEEP_DEG).abs() < 1e-9 && (c.angle(3.0) - SWEEP_DEG).abs() < 1e-9);
    }

    #[test]
    fn a_decibel_card_is_evenly_spaced() {
        let c = Card::Decibels;
        assert!(((c.fraction(-30.0) - c.fraction(-40.0)) - (c.fraction(10.0) - c.fraction(0.0))).abs() < 1e-9);
        assert!(c.hot_from().is_none());
    }

    #[test]
    fn the_scale_uses_the_width_of_the_card() {
        let (l, _) = point(Card::Vu, -20.0, 0.0);
        let (r, _) = point(Card::Vu, 3.0, 0.0);
        let f = (r - l) / VU_W;
        assert!(f > 0.85 && f < 1.0, "{f}");
    }
}
