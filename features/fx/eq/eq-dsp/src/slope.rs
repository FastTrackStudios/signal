//! Pro-Q 4 slope alias system.
//!
//! Pro-Q exposes 10 standard slope values plus an extra Brickwall mode for
//! HP/LP. The internal slope index (0..=9) maps to a dB/octave value the UI
//! displays.
//!
//! Per-filter minimum slope (from FabFilter docs):
//!   - HP / LP / BP: 0 dB/oct
//!   - Bell / Notch: 12 dB/oct
//!   - Shelves / TiltShelf / FlatTilt / AllPass: 6 dB/oct

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

impl Slope {
    /// Pro-Q internal slope index (0..=9). `Brickwall` returns `None`.
    pub fn pro_q_index(self) -> Option<usize> {
        Some(match self {
            Slope::Db0 => 0,
            Slope::Db6 => 1,
            Slope::Db12 => 2,
            Slope::Db18 => 3,
            Slope::Db24 => 4,
            Slope::Db30 => 5,
            Slope::Db36 => 6,
            Slope::Db48 => 7,
            Slope::Db72 => 8,
            Slope::Db96 => 9,
            Slope::Brickwall => return None,
        })
    }

    /// dB/oct as a number. `Brickwall` returns `f64::INFINITY`.
    pub fn db_per_octave(self) -> f64 {
        match self {
            Slope::Db0 => 0.0,
            Slope::Db6 => 6.0,
            Slope::Db12 => 12.0,
            Slope::Db18 => 18.0,
            Slope::Db24 => 24.0,
            Slope::Db30 => 30.0,
            Slope::Db36 => 36.0,
            Slope::Db48 => 48.0,
            Slope::Db72 => 72.0,
            Slope::Db96 => 96.0,
            Slope::Brickwall => f64::INFINITY,
        }
    }

    /// Build a `Slope` from a Pro-Q internal slope index (0..=9).
    pub fn from_index(idx: usize) -> Option<Slope> {
        Some(match idx {
            0 => Slope::Db0,
            1 => Slope::Db6,
            2 => Slope::Db12,
            3 => Slope::Db18,
            4 => Slope::Db24,
            5 => Slope::Db30,
            6 => Slope::Db36,
            7 => Slope::Db48,
            8 => Slope::Db72,
            9 => Slope::Db96,
            _ => return None,
        })
    }

    /// CANONICAL slope param index (0..=10, Brickwall = 10) — the ONE
    /// table every param surface uses (plugin, signal-fx, rig, tests).
    pub fn param_index(self) -> usize {
        match self {
            Slope::Db0 => 0,
            Slope::Db6 => 1,
            Slope::Db12 => 2,
            Slope::Db18 => 3,
            Slope::Db24 => 4,
            Slope::Db30 => 5,
            Slope::Db36 => 6,
            Slope::Db48 => 7,
            Slope::Db72 => 8,
            Slope::Db96 => 9,
            Slope::Brickwall => 10,
        }
    }

    /// Canonical inverse of [`Self::param_index`]. Out-of-range clamps
    /// to Db12 (the historical default order).
    pub fn from_param_index(idx: usize) -> Slope {
        match idx {
            0 => Slope::Db0,
            1 => Slope::Db6,
            2 => Slope::Db12,
            3 => Slope::Db18,
            4 => Slope::Db24,
            5 => Slope::Db30,
            6 => Slope::Db36,
            7 => Slope::Db48,
            8 => Slope::Db72,
            9 => Slope::Db96,
            10 => Slope::Brickwall,
            _ => Slope::Db12,
        }
    }

    /// CANONICAL filter order (pole count) for this slope — replaces
    /// the three divergent tables that lived in the plugin shell and
    /// the conformance tests. Brickwall currently maps to the maximum
    /// IIR order (a dedicated brickwall path is tracked in issue #73).
    pub fn order(self) -> usize {
        match self {
            Slope::Db0 => 0,
            Slope::Db6 => 1,
            Slope::Db12 => 2,
            Slope::Db18 => 3,
            Slope::Db24 => 4,
            Slope::Db30 => 5,
            Slope::Db36 => 6,
            Slope::Db48 => 8,
            Slope::Db72 => 12,
            Slope::Db96 => 16,
            Slope::Brickwall => 16,
        }
    }

    /// Number of biquad sections Pro-Q uses for this slope.
    /// Empirically captured via probe (see capture_grid.py + RE).
    pub fn section_count(self) -> usize {
        match self {
            Slope::Db0 | Slope::Db6 | Slope::Db12 => 1,
            Slope::Db18 | Slope::Db24 => 2,
            Slope::Db30 | Slope::Db36 => 3,
            Slope::Db48 => 4,
            Slope::Db72 => 6,
            Slope::Db96 => 8,
            Slope::Brickwall => 0,
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
    pub fn canonical_index(self) -> u32 {
        self as u32
    }

    /// Canonical inverse. Out-of-range falls back to Bell.
    pub fn from_canonical_index(idx: u32) -> FilterShape {
        match idx {
            0 => FilterShape::Bell,
            1 => FilterShape::LowShelf,
            2 => FilterShape::HighShelf,
            3 => FilterShape::LowCut,
            4 => FilterShape::HighCut,
            5 => FilterShape::Notch,
            6 => FilterShape::BandPass,
            7 => FilterShape::TiltShelf,
            8 => FilterShape::FlatTilt,
            9 => FilterShape::AllPass,
            10 => FilterShape::BandShelf,
            11 => FilterShape::ShelfAlt,
            12 => FilterShape::BandPassVariant,
            _ => FilterShape::Bell,
        }
    }

    /// The DSP design entry point for this UI shape.
    pub fn to_filter_type(self) -> crate::design::FilterType {
        use crate::design::FilterType;
        match self {
            FilterShape::Bell => FilterType::Peak,
            FilterShape::LowShelf => FilterType::LowShelf,
            FilterShape::HighShelf => FilterType::HighShelf,
            FilterShape::LowCut => FilterType::Highpass,
            FilterShape::HighCut => FilterType::Lowpass,
            FilterShape::Notch => FilterType::Notch,
            FilterShape::BandPass => FilterType::Bandpass,
            FilterShape::TiltShelf => FilterType::TiltShelf,
            FilterShape::FlatTilt => FilterType::FlatTilt,
            FilterShape::AllPass => FilterType::Allpass,
            FilterShape::BandShelf => FilterType::BandShelf,
            FilterShape::ShelfAlt => FilterType::ShelfAlt,
            FilterShape::BandPassVariant => FilterType::BandPassVariant,
        }
    }

    /// Effective filter order for a canonical slope param index,
    /// clamped to this shape's minimum slope. The single entry point
    /// for every param surface.
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
        slope.order().max(1)
    }

    /// Minimum dB/oct slope this filter shape allows in Pro-Q UI.
    pub fn min_slope(self) -> Slope {
        match self {
            FilterShape::LowCut | FilterShape::HighCut | FilterShape::BandPass => Slope::Db0,
            FilterShape::Bell | FilterShape::Notch => Slope::Db12,
            FilterShape::LowShelf
            | FilterShape::HighShelf
            | FilterShape::TiltShelf
            | FilterShape::FlatTilt
            | FilterShape::AllPass
            | FilterShape::BandShelf
            | FilterShape::ShelfAlt
            | FilterShape::BandPassVariant => Slope::Db6,
        }
    }

    /// Whether this shape supports Brickwall slope (HP/LP only).
    pub fn supports_brickwall(self) -> bool {
        matches!(self, FilterShape::LowCut | FilterShape::HighCut)
    }

    /// Whether this shape uses the gain parameter.
    pub fn uses_gain(self) -> bool {
        matches!(
            self,
            FilterShape::Bell
                | FilterShape::LowShelf
                | FilterShape::HighShelf
                | FilterShape::TiltShelf
                | FilterShape::FlatTilt
                | FilterShape::BandShelf
                | FilterShape::ShelfAlt
        )
    }

    /// Pro-Q binary filter type ID (probe argv[2]).
    pub fn pro_q_type_id(self) -> u32 {
        match self {
            FilterShape::Bell => 0,
            FilterShape::LowShelf => 1,
            FilterShape::HighCut => 2,
            FilterShape::HighShelf => 3,
            FilterShape::LowCut => 4,
            FilterShape::Notch => 5,
            FilterShape::BandPass => 6,
            FilterShape::TiltShelf => 7,
            FilterShape::FlatTilt => 8,
            FilterShape::AllPass => 11,
            FilterShape::BandShelf => 10,
            FilterShape::ShelfAlt => 12,
            FilterShape::BandPassVariant => 5,
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
        // Cuts honor low slopes.
        assert_eq!(FilterShape::LowCut.effective_order(1), 1);
        // Brickwall only for cuts; others cap at Db96.
        assert_eq!(FilterShape::Bell.effective_order(10), 16);
        assert_eq!(FilterShape::LowCut.effective_order(10), 16);
    }

    #[test]
    fn brickwall_only_for_cuts() {
        assert!(FilterShape::LowCut.supports_brickwall());
        assert!(FilterShape::HighCut.supports_brickwall());
        assert!(!FilterShape::Bell.supports_brickwall());
    }
}
