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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterShape {
    Bell,
    LowShelf,
    LowCut,
    HighShelf,
    HighCut,
    Notch,
    BandPass,
    TiltShelf,
    FlatTilt,
    AllPass,
}

impl FilterShape {
    /// Minimum dB/oct slope this filter shape allows in Pro-Q UI.
    pub fn min_slope(self) -> Slope {
        match self {
            FilterShape::LowCut | FilterShape::HighCut | FilterShape::BandPass => Slope::Db0,
            FilterShape::Bell | FilterShape::Notch => Slope::Db12,
            FilterShape::LowShelf
            | FilterShape::HighShelf
            | FilterShape::TiltShelf
            | FilterShape::FlatTilt
            | FilterShape::AllPass => Slope::Db6,
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
    fn brickwall_only_for_cuts() {
        assert!(FilterShape::LowCut.supports_brickwall());
        assert!(FilterShape::HighCut.supports_brickwall());
        assert!(!FilterShape::Bell.supports_brickwall());
    }
}
