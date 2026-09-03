//! Pro-Q 4 slope alias system.
//!
//! Pro-Q exposes 10 standard slope values plus an extra Brickwall mode for
//! HP/LP. The internal slope index (0..=9) maps to a dB/octave value the UI
//! displays.
//!
//! Per-filter minimum slope (from `FabFilter` docs):
//!   - HP / LP / BP: 0 dB/oct
//!   - Bell / Notch: 12 dB/oct
//!   - Shelves / `TiltShelf` / `FlatTilt` / `AllPass`: 6 dB/oct

/// Pro-Q standard slope settings, named by their dB/oct value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slope {
    /// 0 dB/oct — bypass for HP/LP/BP only.
    Db0,
    /// 6 dB/oct — single-pole.
    Db6,
    /// 12 dB/oct — minimum for Bell/Notch.
    Db12,
    /// 18 dB/oct.
    Db18,
    /// 24 dB/oct.
    Db24,
    /// 30 dB/oct.
    Db30,
    /// 36 dB/oct.
    Db36,
    /// 48 dB/oct.
    Db48,
    /// 72 dB/oct.
    Db72,
    /// 96 dB/oct.
    Db96,
    /// Brickwall — HP/LP only.
    Brickwall,
}

/// The `order` value that means "Brickwall", not a pole count.
///
/// Brickwall is an elliptic design of order 12 (see
/// `design::brickwall`), so no real order can stand for it: the design
/// dispatcher has to be told which family to build, not how many poles to
/// build it from. A value no slope's pole count can collide with says that
/// unambiguously.
pub const BRICKWALL_ORDER: usize = 1000;

impl Slope {
    /// Pro-Q internal slope index (0..=9). `Brickwall` returns `None`.
    #[must_use]
    pub const fn pro_q_index(self) -> Option<usize> {
        Some(match self {
            Self::Db0 => 0,
            Self::Db6 => 1,
            Self::Db12 => 2,
            Self::Db18 => 3,
            Self::Db24 => 4,
            Self::Db30 => 5,
            Self::Db36 => 6,
            Self::Db48 => 7,
            Self::Db72 => 8,
            Self::Db96 => 9,
            Self::Brickwall => return None,
        })
    }

    /// dB/oct as a number. `Brickwall` returns `f64::INFINITY`.
    #[must_use]
    pub const fn db_per_octave(self) -> f64 {
        match self {
            Self::Db0 => 0.0,
            Self::Db6 => 6.0,
            Self::Db12 => 12.0,
            Self::Db18 => 18.0,
            Self::Db24 => 24.0,
            Self::Db30 => 30.0,
            Self::Db36 => 36.0,
            Self::Db48 => 48.0,
            Self::Db72 => 72.0,
            Self::Db96 => 96.0,
            Self::Brickwall => f64::INFINITY,
        }
    }

    /// Build a `Slope` from a Pro-Q internal slope index (0..=9).
    #[must_use]
    pub const fn from_index(idx: usize) -> Option<Self> {
        Some(match idx {
            0 => Self::Db0,
            1 => Self::Db6,
            2 => Self::Db12,
            3 => Self::Db18,
            4 => Self::Db24,
            5 => Self::Db30,
            6 => Self::Db36,
            7 => Self::Db48,
            8 => Self::Db72,
            9 => Self::Db96,
            _ => return None,
        })
    }

    /// CANONICAL slope param index (0..=10, Brickwall = 10) — the ONE
    /// table every param surface uses (plugin, signal-fx, rig, tests).
    #[must_use]
    pub fn param_index(self) -> usize {
        match self {
            Self::Db0 => 0,
            Self::Db6 => 1,
            Self::Db12 => 2,
            Self::Db18 => 3,
            Self::Db24 => 4,
            Self::Db30 => 5,
            Self::Db36 => 6,
            Self::Db48 => 7,
            Self::Db72 => 8,
            Self::Db96 => 9,
            Self::Brickwall => 10,
        }
    }

    /// Canonical inverse of [`Self::param_index`]. Out-of-range clamps
    /// to Db12 (the historical default order).
    #[must_use]
    pub fn from_param_index(idx: usize) -> Self {
        match idx {
            0 => Self::Db0,
            1 => Self::Db6,
            3 => Self::Db18,
            4 => Self::Db24,
            5 => Self::Db30,
            6 => Self::Db36,
            7 => Self::Db48,
            8 => Self::Db72,
            9 => Self::Db96,
            10 => Self::Brickwall,
            _ => Self::Db12,
        }
    }

    /// CANONICAL filter order (pole count) for this slope — replaces
    /// the three divergent tables that lived in the plugin shell and
    /// the conformance tests.
    ///
    /// Brickwall is the exception: it is not a pole count at all, it is a
    /// different *design*, so it returns [`BRICKWALL_ORDER`] as a sentinel
    /// that `design_filter` routes on. It used to return 16 — the same as
    /// 96 dB/oct — which made the plugin's steepest setting and its
    /// second-steepest identical filters, 65 dB apart from the plugin an
    /// eighth of an octave past the corner.
    #[must_use]
    pub fn order(self) -> usize {
        match self {
            Self::Db0 => 0,
            Self::Db6 => 1,
            Self::Db12 => 2,
            Self::Db18 => 3,
            Self::Db24 => 4,
            Self::Db30 => 5,
            Self::Db36 => 6,
            Self::Db48 => 8,
            Self::Db72 => 12,
            Self::Db96 => 16,
            Self::Brickwall => BRICKWALL_ORDER,
        }
    }

    /// Number of biquad sections Pro-Q uses for this slope.
    /// Empirically captured via probe (see `capture_grid.py` + RE).
    #[must_use]
    pub fn section_count(self) -> usize {
        match self {
            Self::Db0 | Self::Db6 | Self::Db12 => 1,
            Self::Db18 | Self::Db24 => 2,
            Self::Db30 | Self::Db36 => 3,
            Self::Db48 => 4,
            Self::Db72 => 6,
            Self::Db96 => 8,
            Self::Brickwall => 0,
        }
    }
}

/// Pro-Q UI filter shape — mirrors the shape button options.
///
/// The variant ORDER here is the **canonical shape index** used by
/// every param surface (signal-fx `b{i}_shape`, the rig patches, the
/// web UI's `EqBandShape::all()`, and the plugin shell). It is
/// APPEND-ONLY: never reorder, only add at the end. The last three
/// variants expose the previously-unreachable DSP designs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FilterShape {
    Bell = 0,
    LowShelf = 1,
    HighShelf = 2,
    LowCut = 3,
    HighCut = 4,
    Notch = 5,
    BandPass = 6,
    TiltShelf = 7,
    FlatTilt = 8,
    AllPass = 9,
    /// Band shelf (Pro-Q type 10) — previously design-only.
    BandShelf = 10,
    /// Alternative shelf cascade (Pro-Q type 12) — previously design-only.
    ShelfAlt = 11,
    /// Bandpass variant (Pro-Q type 5) — previously design-only.
    BandPassVariant = 12,
}

impl FilterShape {
    /// Canonical shape index (the wire/param value).
    #[must_use]
    pub fn canonical_index(self) -> u32 {
        self as u32
    }

    /// Canonical inverse. Out-of-range falls back to Bell.
    #[must_use]
    pub fn from_canonical_index(idx: u32) -> Self {
        match idx {
            1 => Self::LowShelf,
            2 => Self::HighShelf,
            3 => Self::LowCut,
            4 => Self::HighCut,
            5 => Self::Notch,
            6 => Self::BandPass,
            7 => Self::TiltShelf,
            8 => Self::FlatTilt,
            9 => Self::AllPass,
            10 => Self::BandShelf,
            11 => Self::ShelfAlt,
            12 => Self::BandPassVariant,
            _ => Self::Bell,
        }
    }

    /// The DSP design entry point for this UI shape.
    #[must_use]
    pub fn to_filter_type(self) -> crate::design::FilterType {
        use crate::design::FilterType;
        match self {
            Self::Bell => FilterType::Peak,
            Self::LowShelf => FilterType::LowShelf,
            Self::HighShelf => FilterType::HighShelf,
            Self::LowCut => FilterType::Highpass,
            Self::HighCut => FilterType::Lowpass,
            Self::Notch => FilterType::Notch,
            Self::BandPass => FilterType::Bandpass,
            Self::TiltShelf => FilterType::TiltShelf,
            Self::FlatTilt => FilterType::FlatTilt,
            Self::AllPass => FilterType::Allpass,
            Self::BandShelf => FilterType::BandShelf,
            Self::ShelfAlt => FilterType::ShelfAlt,
            Self::BandPassVariant => FilterType::BandPassVariant,
        }
    }

    /// Effective filter order for a canonical slope param index,
    /// clamped to this shape's minimum slope. The single entry point
    /// for every param surface. Returns 0 for a 0 dB/oct cut — that
    /// slope means BYPASS on Low/High Cut and Band Pass (Pro-Q
    /// behavior); callers disable the band.
    #[must_use]
    pub fn effective_order(self, slope_param_index: usize) -> usize {
        let slope = Slope::from_param_index(slope_param_index);
        let min = self.min_slope();
        let slope = if slope.param_index() < min.param_index() {
            min
        } else if slope == Slope::Brickwall && !self.supports_brickwall() {
            Slope::Db96
        } else {
            slope
        };
        slope.order()
    }

    /// Minimum dB/oct slope this filter shape allows in Pro-Q UI.
    #[must_use]
    pub fn min_slope(self) -> Slope {
        match self {
            Self::LowCut | Self::HighCut | Self::BandPass => Slope::Db0,
            Self::Bell | Self::Notch => Slope::Db12,
            Self::LowShelf
            | Self::HighShelf
            | Self::TiltShelf
            | Self::FlatTilt
            | Self::AllPass
            | Self::BandShelf
            | Self::ShelfAlt
            | Self::BandPassVariant => Slope::Db6,
        }
    }

    /// Whether this shape supports Brickwall slope (HP/LP only).
    #[must_use]
    pub fn supports_brickwall(self) -> bool {
        matches!(self, Self::LowCut | Self::HighCut)
    }

    /// Whether this shape uses the gain parameter.
    #[must_use]
    pub fn uses_gain(self) -> bool {
        matches!(
            self,
            Self::Bell
                | Self::LowShelf
                | Self::HighShelf
                | Self::TiltShelf
                | Self::FlatTilt
                | Self::BandShelf
                | Self::ShelfAlt
        )
    }

    /// Pro-Q binary filter type ID (probe argv[2]).
    #[must_use]
    pub fn pro_q_type_id(self) -> u32 {
        match self {
            Self::Bell => 0,
            Self::LowShelf => 1,
            Self::HighCut => 2,
            Self::HighShelf => 3,
            Self::LowCut => 4,
            Self::Notch | Self::BandPassVariant => 5,
            Self::BandPass => 6,
            Self::TiltShelf => 7,
            Self::FlatTilt => 8,
            Self::AllPass => 11,
            Self::BandShelf => 10,
            Self::ShelfAlt => 12,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slope_round_trip() {
        for idx in 0..=9 {
            let s = Slope::from_index(idx).unwrap();
            assert_eq!(s.pro_q_index(), Some(idx));
        }
    }

    #[test]
    fn min_slopes_match_docs() {
        assert_eq!(FilterShape::LowCut.min_slope(), Slope::Db0);
        assert_eq!(FilterShape::Bell.min_slope(), Slope::Db12);
        assert_eq!(FilterShape::LowShelf.min_slope(), Slope::Db6);
    }

    #[test]
    fn canonical_index_round_trips() {
        for i in 0..13u32 {
            let s = FilterShape::from_canonical_index(i);
            assert_eq!(s.canonical_index(), i);
        }
    }

    #[test]
    fn effective_order_clamps_to_shape_minimum() {
        // Bell at slope 0/1 clamps up to Db12 → order 2.
        assert_eq!(FilterShape::Bell.effective_order(0), 2);
        assert_eq!(FilterShape::Bell.effective_order(1), 2);
        // Cuts honor low slopes; 0 dB/oct on a cut = bypass (order 0).
        assert_eq!(FilterShape::LowCut.effective_order(0), 0);
        assert_eq!(FilterShape::LowCut.effective_order(1), 1);
        // Brickwall only for cuts; others cap at Db96 (order 16). A cut gets
        // the sentinel instead, because Brickwall is an elliptic design and
        // not a pole count — see [`BRICKWALL_ORDER`].
        assert_eq!(FilterShape::Bell.effective_order(10), 16);
        assert_eq!(FilterShape::LowCut.effective_order(10), BRICKWALL_ORDER);
    }

    #[test]
    fn brickwall_only_for_cuts() {
        assert!(FilterShape::LowCut.supports_brickwall());
        assert!(FilterShape::HighCut.supports_brickwall());
        assert!(!FilterShape::Bell.supports_brickwall());
    }
}
