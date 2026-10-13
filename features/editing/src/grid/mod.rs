//! A grid that follows the zoom.
//!
//! A fixed grid is wrong at both ends: zoomed out, sixteenths collapse
//! into a grey band; zoomed in, whole notes leave nothing to snap to.
//! The adaptive grid picks the finest division whose lines are still at
//! least a stated distance apart, and re-picks it whenever the view
//! changes.
//!
//! Modelled on Ilias-Timon Poulakis's Adaptive Grid scripts for REAPER
//! (`Reaper-Tools`, MIT), which are the reference implementation of this
//! behaviour and what users of both programs will expect it to feel
//! like. The arithmetic below is theirs; the shape is not, and the
//! difference matters in one place — see [`Adaptive::fit`].
//!
//! ## Where the numbers come from
//!
//! Everything is expressed against **one measure**, because that is the
//! unit a division is a fraction of:
//!
//! - `measure_px` — how wide one measure is on screen, which is the only
//!   thing the caller has to know how to compute. It carries the zoom,
//!   the tempo map and the time signature all at once, so this crate
//!   needs none of them.
//! - [`Density::spacing`] — the closest two gridlines may sit, in
//!   pixels.
//!
//! The finest grid that fits is then `measure_px / spacing` lines per
//! measure, and [`Adaptive::fit`] snaps the *current* grid up or down to
//! it in whole steps of [`Adaptive::factor`].
//!
//! ## Why it snaps by a factor rather than choosing outright
//!
//! Scaling the current division by a power of two keeps everything about
//! it except its size. A triplet grid stays a triplet grid, a dotted one
//! stays dotted, and a project in 6/8 keeps whatever the user chose —
//! because the only thing that changed is how many times it was halved.
//! Choosing a division from a table instead would quietly straighten
//! every grid the first time the user zoomed.

#![forbid(unsafe_code)]

/// How much room the grid is asked to leave between lines.
///
/// The named steps are REAPER's, and are multiples of its `projgridmin`
/// setting — the minimum gridline spacing, 8 pixels by default. They are
/// named rather than numeric because "wide" is the choice a user is
/// making; the pixels are how it is implemented.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Density {
    /// Not adaptive: the division is whatever it was set to.
    Fixed,
    Narrowest,
    Narrow,
    Medium,
    Wide,
    Widest,
    /// An exact spacing in pixels.
    Custom(f64),
}

impl Density {
    /// The multiplier REAPER applies to `projgridmin` for each step.
    fn multiplier(self) -> Option<f64> {
        match self {
            Density::Fixed | Density::Custom(_) => None,
            Density::Narrowest => Some(1.0),
            Density::Narrow => Some(2.0),
            Density::Medium => Some(3.0),
            Density::Wide => Some(4.0),
            Density::Widest => Some(6.0),
        }
    }

    /// The closest two gridlines may sit, in pixels.
    ///
    /// `min_px` is the host's minimum gridline spacing — REAPER's
    /// `projgridmin`, and [`DEFAULT_MIN_PX`] where there is no such
    /// setting to read.
    ///
    /// The extra multiplier's worth of pixels is not a fudge: a gridline
    /// is itself a pixel wide, so *n* divisions cost *n* pixels of ink
    /// that are not gaps. REAPER accounts for it the same way, and
    /// without it the widest settings creep one step too fine.
    pub fn spacing(self, min_px: f64) -> Option<f64> {
        match self {
            Density::Fixed => None,
            Density::Custom(px) => Some(px + 1.0),
            _ => self.multiplier().map(|m| min_px * m + m),
        }
    }

    /// Every named density, coarsest first — for a menu.
    pub const NAMED: [Density; 5] = [
        Density::Widest,
        Density::Wide,
        Density::Medium,
        Density::Narrow,
        Density::Narrowest,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Density::Fixed => "Fixed",
            Density::Narrowest => "Narrowest",
            Density::Narrow => "Narrow",
            Density::Medium => "Medium",
            Density::Wide => "Wide",
            Density::Widest => "Widest",
            Density::Custom(_) => "Custom",
        }
    }
}

/// The host's minimum gridline spacing where there is none to read.
///
/// REAPER's own default for `projgridmin`.
pub const DEFAULT_MIN_PX: f64 = 8.0;

/// The adaptive grid's settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Adaptive {
    pub density: Density,
    /// The host's minimum gridline spacing, in pixels.
    pub min_px: f64,
    /// What the division is multiplied or divided by per step.
    ///
    /// Two — halving and doubling — is the only value that lands on
    /// divisions a musician names. Four steps in bigger jumps for a
    /// coarser feel, and REAPER exposes it, so it is exposed here.
    pub factor: f64,
    /// The finest division the grid may reach, as a fraction of a whole
    /// note. Nothing smaller is useful and something has gone wrong if
    /// it is asked for.
    pub min_division: f64,
    /// The coarsest division the grid may reach.
    pub max_division: f64,
}

impl Default for Adaptive {
    /// Adaptive, at the middle density.
    ///
    /// It was `Fixed` — the grid did nothing until asked — which is the
    /// safe default and the wrong one. A fixed grid is *always* wrong at
    /// some zoom: at 1/16 the lines become a grey wash the moment you
    /// zoom out to look at a section, and coarse enough to read a whole
    /// arrangement is useless the moment you go back in. Following the
    /// zoom is what makes the grid mean the same thing at every scale,
    /// and the setting people reach for is the division they want when
    /// zoomed *in* — which is exactly what this keeps as the ceiling.
    ///
    /// Only ever coarsens, never past the setting, so nothing here can
    /// snap a note somewhere finer than was asked for.
    fn default() -> Self {
        Self {
            density: Density::Medium,
            min_px: DEFAULT_MIN_PX,
            factor: 2.0,
            // A 4096th and eight whole notes: REAPER's own limits, which
            // exist to stop a degenerate zoom producing a division no
            // arithmetic downstream expects.
            min_division: 1.0 / 4096.0,
            max_division: 8.0,
        }
    }
}

impl Adaptive {
    /// Whether the grid follows the zoom at all.
    pub fn is_adaptive(&self) -> bool {
        self.density != Density::Fixed
    }

    /// The division to use, given the finest the user wants and how wide
    /// a measure is on screen.
    ///
    /// `finest` and the result are fractions of a whole note: `0.25` is
    /// a quarter note, `1.0/16.0` a sixteenth. Returns `None` when the
    /// grid is fixed, or when the view is too degenerate to say anything
    /// about — a zero-width measure has no answer, and guessing one
    /// would move the user's grid for no reason.
    ///
    /// ## The setting is a ceiling
    ///
    /// **The result is never finer than `finest`.** The division the
    /// user picked is the most detail they ever want; zooming out
    /// coarsens it and zooming back in returns to it, and no amount of
    /// zooming produces a grid they did not ask for.
    ///
    /// This is where the behaviour departs from the REAPER scripts,
    /// deliberately. Those scale whatever the grid *currently* is, so
    /// the setting is a starting point that the zoom then walks away
    /// from in both directions — zoom in far enough and you are snapping
    /// to 1/512 having asked for 1/16. Anchoring on the user's choice
    /// instead makes the control mean one thing, and makes the result a
    /// pure function of (setting, zoom) rather than of how you got here.
    ///
    /// ## What it preserves
    ///
    /// The result is always `finest` times a power of [`Self::factor`],
    /// which is what carries triplet and dotted grids through a zoom
    /// unchanged. Where the reference scripts have to recover a grid's
    /// type from the division itself — 1/6 being a triplet eighth — that
    /// is free here, because a caller holding `triplet` as its own flag
    /// never encoded the type in the number to begin with.
    pub fn fit(&self, finest: f64, measure_px: f64) -> Option<f64> {
        let spacing = self.density.spacing(self.min_px)?;
        if !(measure_px.is_finite() && measure_px > 0.0)
            || !(finest.is_finite() && finest > 0.0)
            || !(spacing > 0.0)
            || self.factor <= 1.0
        {
            return None;
        }

        // Lines per measure: the most that fit, and how many the user
        // asked for. A division is a fraction of a whole note, so its
        // reciprocal is how many of it make one.
        let most = measure_px / spacing;
        let wanted = 1.0 / finest;
        if most <= 0.0 {
            return None;
        }

        // How many whole steps of `factor` separate the two, never
        // positive: a step up would be finer than the user asked for.
        // Flooring keeps the result *at least* `spacing` apart rather
        // than merely close to it.
        let steps = (most / wanted).log(self.factor).floor().min(0.0);
        if !steps.is_finite() {
            return None;
        }
        let fitted = 1.0 / (wanted * self.factor.powf(steps));

        // Only ever coarser, so the floor cannot be crossed from here
        // and the ceiling is the one limit left to enforce.
        if fitted > self.max_division {
            return Some(self.max_division.max(finest));
        }
        Some(fitted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adaptive(density: Density) -> Adaptive {
        Adaptive {
            density,
            ..Default::default()
        }
    }

    /// The point of the feature: zooming out coarsens the grid.
    #[test]
    fn zooming_out_coarsens_the_grid() {
        let a = adaptive(Density::Medium);
        // A measure 800px wide leaves room for a fine grid.
        let wide = a.fit(1.0 / 16.0, 800.0).unwrap();
        // The same grid with the measure squeezed to 80px cannot.
        let tight = a.fit(1.0 / 16.0, 80.0).unwrap();
        assert!(
            tight > wide,
            "zoomed out should be coarser: {tight} vs {wide}"
        );
    }

    /// And no line is ever closer than the density asked for.
    #[test]
    fn the_lines_are_never_closer_than_asked() {
        let a = adaptive(Density::Medium);
        let spacing = Density::Medium.spacing(DEFAULT_MIN_PX).unwrap();
        for measure_px in [40.0, 137.0, 400.0, 1600.0, 9000.0] {
            let d = a.fit(1.0 / 16.0, measure_px).unwrap();
            let gap = measure_px * d;
            assert!(
                gap >= spacing - 1e-9,
                "at {measure_px}px a measure, lines sat {gap}px apart, under {spacing}"
            );
        }
    }

    /// A coarser density leaves more room than a finer one.
    #[test]
    fn density_orders_as_its_names_do() {
        let mut previous = 0.0;
        for density in [
            Density::Narrowest,
            Density::Narrow,
            Density::Medium,
            Density::Wide,
            Density::Widest,
        ] {
            let d = adaptive(density).fit(1.0 / 16.0, 600.0).unwrap();
            assert!(
                d >= previous,
                "{} should be no finer than the step below it",
                density.label()
            );
            previous = d;
        }
    }

    /// The division is scaled, so a grid keeps whatever it was.
    ///
    /// This is the property that lets a caller hold `triplet` as its own
    /// flag: every result is the input times a power of the factor, so a
    /// third stays a third.
    #[test]
    fn a_fitted_division_is_the_old_one_scaled_by_the_factor() {
        let a = adaptive(Density::Medium);
        for start in [1.0 / 16.0, 1.0 / 24.0, 1.0 / 12.0, 3.0 / 16.0] {
            for measure_px in [60.0, 250.0, 1000.0, 4000.0] {
                let fitted = a.fit(start, measure_px).unwrap();
                let ratio = (fitted / start).log2();
                assert!(
                    (ratio - ratio.round()).abs() < 1e-9,
                    "{start} -> {fitted} is not a power of two apart"
                );
            }
        }
    }

    #[test]
    fn a_fixed_grid_is_left_alone() {
        assert_eq!(adaptive(Density::Fixed).fit(1.0 / 16.0, 500.0), None);
    }

    /// A degenerate view has no answer, and must not invent one.
    #[test]
    fn a_view_with_no_width_moves_nothing() {
        let a = adaptive(Density::Medium);
        assert_eq!(a.fit(1.0 / 16.0, 0.0), None);
        assert_eq!(a.fit(1.0 / 16.0, f64::NAN), None);
        assert_eq!(a.fit(0.0, 500.0), None);
    }

    /// The setting is a ceiling: zooming in never goes past it.
    ///
    /// The control means "the most detail I want", so no zoom produces a
    /// finer grid than the one asked for — which is the whole difference
    /// between this and scaling whatever the grid happens to be.
    #[test]
    fn the_setting_is_the_finest_the_grid_ever_gets() {
        let a = adaptive(Density::Medium);
        let finest = 1.0 / 16.0;
        // A measure a hundred thousand pixels wide has room for
        // 1/4096ths. It must still give back 1/16.
        assert_eq!(a.fit(finest, 100_000.0), Some(finest));
        // And at a merely generous zoom, likewise.
        assert_eq!(a.fit(finest, 2_000.0), Some(finest));
    }

    /// Zooming out and back in returns to exactly the setting.
    ///
    /// It can, because the result is a function of the setting and the
    /// zoom rather than of the grid's own history — there is no drift to
    /// accumulate.
    #[test]
    fn the_grid_comes_back_to_where_it_started() {
        let a = adaptive(Density::Medium);
        let finest = 1.0 / 16.0;
        let out = a.fit(finest, 50.0).unwrap();
        assert!(out > finest, "zooming out should have coarsened it");
        assert_eq!(a.fit(finest, 4_000.0), Some(finest));
    }

    /// Custom spacing is honoured exactly.
    #[test]
    fn custom_spacing_is_in_pixels() {
        // +1 for the gridline's own pixel, as REAPER does.
        assert_eq!(Density::Custom(24.0).spacing(DEFAULT_MIN_PX), Some(25.0));
    }
}
