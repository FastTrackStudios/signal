//! The shapes a host's FX column gives a compressor: a lane one track tall
//! holds two units stacked (each [`Shape::Lane`]) or one alone
//! ([`Shape::Tall`]). Each unit is redrawn for the shape — the same plate,
//! knobs, meter and lettering as its rack face, laid out for the room —
//! and binds the rack face's own addresses, so a host drives every shape
//! with one set of params.

/// The two shapes a lane gives a unit.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shape {
    /// One of two stacked in the lane: 760 × 164.
    Lane,
    /// Alone in the lane: 760 × 328.
    Tall,
}

impl Shape {
    /// Both shapes, lane first.
    pub const ALL: [Self; 2] = [Self::Lane, Self::Tall];

    /// The face's size.
    #[must_use]
    pub const fn size(self) -> (f64, f64) {
        match self {
            Self::Lane => (760.0, 164.0),
            Self::Tall => (760.0, 328.0),
        }
    }

    /// The suffix on the rack face's file name.
    #[must_use]
    pub const fn suffix(self) -> &'static str {
        match self {
            Self::Lane => "lane",
            Self::Tall => "tall",
        }
    }

    /// `base`'s file at this shape: `<base>-lane` / `<base>-tall`.
    #[must_use]
    pub fn file(self, base: &str) -> &'static str {
        Box::leak(format!("{base}-{}", self.suffix()).into_boxed_str())
    }
}
